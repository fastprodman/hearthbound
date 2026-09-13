use crate::{GameMap, GridPosition, UnitId, WorldPosition, find_path};
use protocol::{ClientCommand, PlayerId, ServerMessage, UnitState};
use std::{
    collections::{HashSet, HashMap, VecDeque},
    time::Duration,
};

pub const SERVER_TICK: Duration = Duration::from_millis(50);

pub const DEFAULT_UNIT_STATS: UnitStats = UnitStats {
    base_move_speed: 2.0,
    base_max_health: 100,
    base_attack_damage: 10,
    base_attack_range: 1.5,
    base_attack_interval: Duration::from_millis(500),
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Health {
    current: u32,
    maximum: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveError {
    UnitNotFound(UnitId),
    NoPath {
        start: GridPosition,
        goal: GridPosition,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    EmptyUnitSelection,
    Move(MoveError),
    Attack(AttackError),
    UnitNotOwned(UnitId),
}

impl From<MoveError> for CommandError {
    fn from(error: MoveError) -> Self {
        Self::Move(error)
    }
}

impl From<AttackError> for CommandError {
    fn from(error: AttackError) -> Self {
        Self::Attack(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackError {
    AttackerNotFound(UnitId),
    TargetNotFound(UnitId),
    FriendlyTarget(UnitId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Movement {
    pub path: VecDeque<GridPosition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub id: UnitId,
    pub owner: PlayerId,
    pub position: WorldPosition,
    pub movement: Option<Movement>,
    pub health: Health,
    pub attack_target: Option<UnitId>,

    pub stats: UnitStats,

    attack_cooldown: Duration,
}

impl Unit {
    pub fn attack_damage(&self) -> u32 {
        self.stats.base_attack_damage
    }

    pub fn attack_range(&self) -> f32 {
        self.stats.base_attack_range
    }

    pub fn attack_interval(&self) -> Duration {
        self.stats.base_attack_interval
    }

    pub fn move_speed(&self) -> f32 {
        self.stats.base_move_speed
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitStats {
    pub base_move_speed: f32,
    pub base_max_health: u32,
    pub base_attack_damage: u32,
    pub base_attack_range: f32,
    pub base_attack_interval: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    PositionNotWalkable(GridPosition),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEvent {
    UnitDied{
        unit: UnitId,
    },
}

#[derive(Debug)]
pub struct GameWorld {
    map: GameMap,
    units: HashMap<UnitId, Unit>,
    next_unit_id: u64,
}

impl GameWorld {
    pub fn new(map: GameMap) -> Self {
        Self {
            map,
            units: HashMap::new(),
            next_unit_id: 1,
        }
    }

    pub fn map(&self) -> &GameMap {
        &self.map
    }

    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        self.units.get(&id)
    }

    pub fn spawn_unit(
        &mut self,
        owner: PlayerId,
        position: GridPosition,
    ) -> Result<UnitId, SpawnError> {
        if !self.map.is_walkable(position) {
            return Err(SpawnError::PositionNotWalkable(position));
        }

        let id = UnitId(self.next_unit_id);

        self.next_unit_id = self
            .next_unit_id
            .checked_add(1)
            .expect("unit ID space exhausted");

        let unit = Unit {
            id,
            owner,
            position: WorldPosition::new(position.x as f32, position.y as f32),
            movement: None,
            health: Health::new(DEFAULT_UNIT_STATS.base_max_health),
            attack_target: None,
            attack_cooldown: Duration::ZERO,
            stats: DEFAULT_UNIT_STATS,
        };

        self.units.insert(id, unit);

        Ok(id)
    }

    pub fn handle_command(
        &mut self,
        owner: PlayerId,
        commnad: ClientCommand,
    ) -> Result<(), CommandError> {
        match commnad {
            ClientCommand::MoveUnits { units, destination } => {
                if units.is_empty() {
                    return Err(CommandError::EmptyUnitSelection);
                }

                // Validate and calculate every path before modifying units.
                let mut planned_movements = Vec::with_capacity(units.len());

                for id in units {
                    let path = self.plan_move(id, owner, destination)?;
                    planned_movements.push((id, path));
                }

                // All movement requests are valid, so they can now be applied.
                for (id, path) in planned_movements {
                    self.apply_movement(id, path);
                }

                Ok(())
            }
            ClientCommand::Attack { attackers, unit } => self.handle_attack_command(owner, attackers, unit),
        }
    }

    fn handle_attack_command(
        &mut self,
        player: PlayerId,
        attackers: Vec<UnitId>,
        target_id: UnitId,
    ) -> Result<(), CommandError> {
        if attackers.is_empty() {
            return Err(CommandError::EmptyUnitSelection);
        }

        for attacker_id in attackers.iter() {
            let attacker = self
                .units
                .get(attacker_id)
                .ok_or(AttackError::AttackerNotFound(*attacker_id))?;

            if attacker.owner != player {
                return Err(CommandError::UnitNotOwned(*attacker_id));
            }

            if attacker.health.is_dead() {
                return Err(AttackError::AttackerNotFound(*attacker_id).into());
            }
        }

        let target_unit = self
            .units
            .get(&target_id)
            .ok_or(AttackError::TargetNotFound(target_id))?;

        if target_unit.owner == player {
            return Err(AttackError::FriendlyTarget(target_id).into());
        }

        if target_unit.health.is_dead() {
            return Err(AttackError::TargetNotFound(target_id).into());
        }

        for attacker_id in attackers {
            let attacker = self
                .units
                .get_mut(&attacker_id)
                .expect("attacker was already validated");

            attacker.attack_target = Some(target_id);

            attacker.movement = None;
        }

        Ok(())
    }

    fn plan_move(
        &self,
        id: UnitId,
        player: PlayerId,
        goal: GridPosition,
    ) -> Result<VecDeque<GridPosition>, CommandError> {
        let unit = self.units.get(&id).ok_or(MoveError::UnitNotFound(id))?;

        if unit.owner != player {
            return Err(CommandError::UnitNotOwned(id));
        }

        let start = GridPosition::new(
            unit.position.x.round() as i32,
            unit.position.y.round() as i32,
        );

        let path = find_path(&self.map, start, goal).ok_or(MoveError::NoPath { start, goal })?;

        Ok(path)
    }

    fn apply_movement(&mut self, id: UnitId, path: VecDeque<GridPosition>) {
        let unit = self
            .units
            .get_mut(&id)
            .expect("movement was already validated");

        unit.movement = if path.is_empty() {
            None
        } else {
            Some(Movement { path })
        };

        unit.attack_target = None;
    }

    pub fn tick(&mut self, delta: Duration) -> Vec<GameEvent> {
        let mut events = Vec::new();

        for unit in self.units.values_mut() {
            tick_unit(unit, delta);

            unit.attack_cooldown = unit.attack_cooldown.saturating_sub(delta);
        }

        self.resolve_attacks();
        events.extend(self.handle_dead_units());

        events
    }

    pub fn snapshot(&self) -> ServerMessage {
        let mut units: Vec<UnitState> = self
            .units
            .values()
            .map(|unit| UnitState {
                id: unit.id,
                owner: unit.owner,
                position: unit.position,
                health: unit.health.current(),
                max_health: unit.health.maximum(),
            })
            .collect();

        units.sort_by_key(|unit| unit.id.0);

        ServerMessage::WorldSnapshot { units: units }
    }

    fn resolve_attacks(&mut self){
        let attacks: Vec<(UnitId, UnitId)> = self.units
            .values()
            .filter_map(|attacker|{
                if attacker.health.is_dead() {
                    return None;
                }

                if !attacker.attack_cooldown.is_zero(){
                    return None;
                }

                let Some(target_id) = attacker.attack_target else {
                    return None;
                };

                let Some(target) = self.units.get(&target_id) else {
                    return None
                };

                if target.health.is_dead() {
                    return None;
                }

                let dx = target.position.x - attacker.position.x;
                let dy = target.position.y - attacker.position.y;
                let distance_squared = dx.powi(2) + dy.powi(2);
                let range_squared = attacker.attack_range().powi(2);

                if distance_squared > range_squared {
                    return None;
                }

                Some((attacker.id, target_id))
            })
            .collect();

        for (attacker_id, target_id) in attacks {
            let attacker = self.units.get_mut(&attacker_id)
                .expect("attacker was collected from the world");

            attacker.attack_cooldown = attacker.attack_interval();
            let attacker_damage = attacker.attack_damage();

            let target = self.units.get_mut(&target_id)
                .expect("target was collected from the world");

            target.health.apply_damage(attacker_damage);
        }
    }

    fn handle_dead_units(&mut self) -> Vec<GameEvent> {
        let dead_units = self.collect_dead_units();

        if dead_units.is_empty() {
            return Vec::new();
        }

        let events: Vec<GameEvent> = dead_units.iter().copied()
            .map(|unit| GameEvent::UnitDied { unit }).collect();

        self.clear_targets_referencing(&dead_units);
        self.remove_units(&dead_units);

        events
    }

    fn collect_dead_units(&self) -> Vec<UnitId> {
        let dead_units = self.units
            .iter()
            .filter_map(|(&id, unit)| {
                if unit.health.is_dead() { 
                    Some(id) 
                } else { 
                    None 
                }
            })
            .collect();

        dead_units
    }

    fn clear_targets_referencing(&mut self, removed: &[UnitId]) {
        let removed: HashSet<UnitId> = removed.iter().copied().collect();
        for unit in self.units.values_mut() {
            if unit
                .attack_target
                .is_some_and(|target| removed.contains(&target)) {
                    unit.attack_target = None;
            }
        }

    }

    fn remove_units(&mut self, removed: &[UnitId]) {
        for &unit_id in removed {
            self.units.remove(&unit_id);
        }
    }
}

impl Health {
    pub const fn new(maximum: u32) -> Self {
        Self {
            current: maximum,
            maximum: maximum,
        }
    }

    pub const fn current(&self) -> u32 {
        self.current
    }

    pub const fn maximum(&self) -> u32 {
        self.maximum
    }

    pub const fn is_dead(&self) -> bool {
        self.current == 0
    }

    pub fn apply_damage(&mut self, amount: u32) {
        self.current = self.current.saturating_sub(amount);
    }
}

fn tick_unit(unit: &mut Unit, delta: Duration) {
    let Some(target_grid) = unit
        .movement
        .as_ref()
        .and_then(|movement| movement.path.front())
        .copied()
    else {
        unit.movement = None;
        return;
    };

    let target = WorldPosition::new(target_grid.x as f32, target_grid.y as f32);

    let dx = target.x - unit.position.x;
    let dy = target.y - unit.position.y;
    let distance = (dx * dx + dy * dy).sqrt();
    let maximum_step = unit.move_speed() * delta.as_secs_f32();

    if distance <= maximum_step {
        unit.position = target;

        let movement = unit.movement.as_mut().expect("movement was checked above");

        movement.path.pop_front();

        if movement.path.is_empty() {
            unit.movement = None;
        }
    } else {
        unit.position.x += dx / distance * maximum_step;
        unit.position.y += dy / distance * maximum_step;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Terrain;
    use protocol::ClientCommand;

    const PLAYER: PlayerId = PlayerId(1);
    const OTHER_PLAYER: PlayerId = PlayerId(2);

    #[test]
    fn spawns_a_unit_on_walkable_terrain() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let id = world
            .spawn_unit(PLAYER, GridPosition::new(2, 3))
            .expect("spawn position should be walkable");

        let unit = world.unit(id).expect("spawned unit should exist");

        assert_eq!(unit.id, id);
        assert_eq!(unit.position, WorldPosition::new(2.0, 3.0));
    }

    #[test]
    fn assigns_unique_unit_ids() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let first = world.spawn_unit(PLAYER, GridPosition::new(1, 1)).unwrap();
        let second = world.spawn_unit(PLAYER, GridPosition::new(2, 1)).unwrap();

        assert_ne!(first, second);
    }

    #[test]
    fn rejects_a_unit_spawned_on_water() {
        let mut map = GameMap::new(8, 8, Terrain::Grass);
        let water = GridPosition::new(3, 2);

        map.set_terrain(water, Terrain::Water).unwrap();

        let mut world = GameWorld::new(map);

        assert_eq!(
            world.spawn_unit(PLAYER, water),
            Err(SpawnError::PositionNotWalkable(water))
        );
    }

    #[test]
    fn unit_reaches_its_destination() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let id = world.spawn_unit(PLAYER, GridPosition::new(0, 0)).unwrap();

        world
            .handle_command(
                PLAYER,
                ClientCommand::MoveUnits {
                    units: vec![id],
                    destination: GridPosition::new(4, 3),
                },
            )
            .unwrap();

        for _ in 0..100 {
            world.tick(SERVER_TICK);
        }

        let unit = world.unit(id).unwrap();

        assert_eq!(unit.position, WorldPosition::new(4.0, 3.0));
        assert_eq!(unit.movement, None);
    }

    #[test]
    fn movement_to_water_is_rejected() {
        let mut map = GameMap::new(8, 8, Terrain::Grass);
        let water = GridPosition::new(3, 2);
        map.set_terrain(water, Terrain::Water).unwrap();

        let mut world = GameWorld::new(map);
        let id = world.spawn_unit(PLAYER, GridPosition::new(0, 0)).unwrap();

        assert_eq!(
            world.handle_command(
                PLAYER,
                ClientCommand::MoveUnits {
                    units: vec![id],
                    destination: water,
                }
            ),
            Err(CommandError::Move(MoveError::NoPath {
                start: GridPosition::new(0, 0),
                goal: water,
            }))
        );
    }

    #[test]
    fn rejects_an_empty_move_command() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        assert_eq!(
            world.handle_command(
                PLAYER,
                ClientCommand::MoveUnits {
                    units: vec![],
                    destination: GridPosition::new(2, 2),
                }
            ),
            Err(CommandError::EmptyUnitSelection)
        );
    }

    #[test]
    fn invalid_group_command_does_not_move_valid_unit() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let valid_id = world.spawn_unit(PLAYER, GridPosition::new(0, 0)).unwrap();

        let missing_id = UnitId(999);

        assert_eq!(
            world.handle_command(
                PLAYER,
                ClientCommand::MoveUnits {
                    units: vec![valid_id, missing_id],
                    destination: GridPosition::new(4, 3),
                }
            ),
            Err(CommandError::Move(MoveError::UnitNotFound(missing_id)))
        );

        assert_eq!(world.unit(valid_id).unwrap().movement, None);
    }

    #[test]
    fn snapshot_contains_authoritative_unit_state() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let first = world.spawn_unit(PLAYER, GridPosition::new(1, 2)).unwrap();

        let second = world.spawn_unit(PLAYER, GridPosition::new(4, 5)).unwrap();

        assert_eq!(
            world.snapshot(),
            ServerMessage::WorldSnapshot {
                units: vec![
                    UnitState {
                        id: first,
                        owner: PLAYER,
                        position: WorldPosition::new(1.0, 2.0),
                        health: DEFAULT_UNIT_STATS.base_max_health,
                        max_health: DEFAULT_UNIT_STATS.base_max_health,
                    },
                    UnitState {
                        id: second,
                        owner: PLAYER,
                        position: WorldPosition::new(4.0, 5.0),
                        health: DEFAULT_UNIT_STATS.base_max_health,
                        max_health: DEFAULT_UNIT_STATS.base_max_health,
                    },
                ],
            }
        );
    }

    #[test]
    fn new_unit_has_full_health() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let id = world.spawn_unit(PLAYER, GridPosition::new(1, 1)).unwrap();

        let unit = world.unit(id).unwrap();

        assert_eq!(unit.health.current(), DEFAULT_UNIT_STATS.base_max_health);
        assert_eq!(unit.health.maximum(), DEFAULT_UNIT_STATS.base_max_health);
        assert!(!unit.health.is_dead());
    }

    #[test]
    fn damage_cannot_reduce_health_below_zero() {
        let mut health = Health::new(100);

        health.apply_damage(150);

        assert_eq!(health.current(), 0);
        assert!(health.is_dead());
    }

    #[test]
    fn attack_sets_enemy_as_target() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let attacker = world
            .spawn_unit(PLAYER, GridPosition::new(1, 1))
            .unwrap();

        let target = world
            .spawn_unit(OTHER_PLAYER, GridPosition::new(2, 1))
            .unwrap();

        world
            .handle_command(
                PLAYER,
                ClientCommand::Attack {
                    attackers: vec![attacker],
                    unit: target,
                },
            )
            .unwrap();

        assert_eq!(
            world.unit(attacker).unwrap().attack_target,
            Some(target)
        );
    }

    #[test]
    fn player_cannot_command_another_players_attacker() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let attacker = world
            .spawn_unit(OTHER_PLAYER, GridPosition::new(1, 1))
            .unwrap();

        let target = world
            .spawn_unit(PLAYER, GridPosition::new(2, 1))
            .unwrap();

        assert_eq!(
            world.handle_command(
                PLAYER,
                ClientCommand::Attack {
                    attackers: vec![attacker],
                    unit: target,
                },
            ),
            Err(CommandError::UnitNotOwned(attacker))
        );

        assert_eq!(world.unit(attacker).unwrap().attack_target, None);
    }

    #[test]
    fn cannot_attack_friendly_unit() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let attacker = world
            .spawn_unit(PLAYER, GridPosition::new(1, 1))
            .unwrap();

        let target = world
            .spawn_unit(PLAYER, GridPosition::new(2, 1))
            .unwrap();

        assert_eq!(
            world.handle_command(
                PLAYER,
                ClientCommand::Attack {
                    attackers: vec![attacker],
                    unit: target,
                },
            ),
            Err(CommandError::Attack(
                AttackError::FriendlyTarget(target)
            ))
        );

        assert_eq!(world.unit(attacker).unwrap().attack_target, None);
    }

    #[test]
    fn attack_rejects_missing_attacker() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let missing_attacker = UnitId(999);

        let target = world
            .spawn_unit(OTHER_PLAYER, GridPosition::new(2, 1))
            .unwrap();

        assert_eq!(
            world.handle_command(
                PLAYER,
                ClientCommand::Attack {
                    attackers: vec![missing_attacker],
                    unit: target,
                },
            ),
            Err(CommandError::Attack(
                AttackError::AttackerNotFound(missing_attacker)
            ))
        );
    }

    #[test]
    fn attack_rejects_missing_target() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let attacker = world
            .spawn_unit(PLAYER, GridPosition::new(1, 1))
            .unwrap();

        let missing_target = UnitId(999);

        assert_eq!(
            world.handle_command(
                PLAYER,
                ClientCommand::Attack {
                    attackers: vec![attacker],
                    unit: missing_target,
                },
            ),
            Err(CommandError::Attack(
                AttackError::TargetNotFound(missing_target)
            ))
        );

        assert_eq!(world.unit(attacker).unwrap().attack_target, None);
    }

    #[test]
    fn invalid_group_attack_does_not_update_valid_attacker() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let valid_attacker = world
            .spawn_unit(PLAYER, GridPosition::new(1, 1))
            .unwrap();

        let foreign_attacker = world
            .spawn_unit(OTHER_PLAYER, GridPosition::new(1, 2))
            .unwrap();

        let target = world
            .spawn_unit(OTHER_PLAYER, GridPosition::new(2, 1))
            .unwrap();

        assert_eq!(
            world.handle_command(
                PLAYER,
                ClientCommand::Attack {
                    attackers: vec![valid_attacker, foreign_attacker],
                    unit: target,
                },
            ),
            Err(CommandError::UnitNotOwned(foreign_attacker))
        );

        assert_eq!(
            world.unit(valid_attacker).unwrap().attack_target,
            None
        );
    }

    #[test]
    fn in_range_attacker_damages_target_on_tick() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let attacker = world
            .spawn_unit(PLAYER, GridPosition::new(1, 1))
            .unwrap();

        let target = world
            .spawn_unit(OTHER_PLAYER, GridPosition::new(2, 1))
            .unwrap();

        world
            .handle_command(
                PLAYER,
                ClientCommand::Attack {
                    attackers: vec![attacker],
                    unit: target,
                },
            )
            .unwrap();

        world.tick(SERVER_TICK);

        assert_eq!(
            world.unit(target).unwrap().health.current(),
            DEFAULT_UNIT_STATS.base_max_health - DEFAULT_UNIT_STATS.base_attack_damage,
        );
    }

    #[test]
    fn attack_respects_attack_interval() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let attacker = world
            .spawn_unit(PLAYER, GridPosition::new(1, 1))
            .unwrap();

        let target = world
            .spawn_unit(OTHER_PLAYER, GridPosition::new(2, 1))
            .unwrap();

        world
            .handle_command(
                PLAYER,
                ClientCommand::Attack {
                    attackers: vec![attacker],
                    unit: target,
                },
            )
            .unwrap();

        // The first available attack happens immediately.
        world.tick(SERVER_TICK);

        assert_eq!(
            world.unit(target).unwrap().health.current(),
            DEFAULT_UNIT_STATS.base_max_health
                - DEFAULT_UNIT_STATS.base_attack_damage
        );

        // The interval has not completely elapsed.
        world.tick(Duration::from_millis(499));

        assert_eq!(
            world.unit(target).unwrap().health.current(),
            DEFAULT_UNIT_STATS.base_max_health
                - DEFAULT_UNIT_STATS.base_attack_damage
        );

        // The final millisecond completes the cooldown.
        world.tick(Duration::from_millis(1));

        assert_eq!(
            world.unit(target).unwrap().health.current(),
            DEFAULT_UNIT_STATS.base_max_health
                - DEFAULT_UNIT_STATS.base_attack_damage * 2
        );
    }

    #[test]
    fn out_of_range_attacker_does_not_damage_target() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let attacker = world
            .spawn_unit(PLAYER, GridPosition::new(0, 0))
            .unwrap();

        let target = world
            .spawn_unit(OTHER_PLAYER, GridPosition::new(3, 0))
            .unwrap();

        world
            .handle_command(
                PLAYER,
                ClientCommand::Attack {
                    attackers: vec![attacker],
                    unit: target,
                },
            )
            .unwrap();

        world.tick(SERVER_TICK);

        assert_eq!(
            world.unit(target).unwrap().health.current(),
            DEFAULT_UNIT_STATS.base_max_health
        );

        assert_eq!(
            world.unit(attacker).unwrap().attack_target,
            Some(target)
        );
    }

    #[test]
    fn move_command_cancels_attack_target() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let attacker = world
            .spawn_unit(PLAYER, GridPosition::new(0, 0))
            .unwrap();

        let target = world
            .spawn_unit(OTHER_PLAYER, GridPosition::new(1, 0))
            .unwrap();

        world
            .handle_command(
                PLAYER,
                ClientCommand::Attack {
                    attackers: vec![attacker],
                    unit: target,
                },
            )
            .unwrap();

        world
            .handle_command(
                PLAYER,
                ClientCommand::MoveUnits {
                    units: vec![attacker],
                    destination: GridPosition::new(0, 2),
                },
            )
            .unwrap();

        let unit = world.unit(attacker).unwrap();

        assert_eq!(unit.attack_target, None);
        assert!(unit.movement.is_some());
    }

    #[test]
    fn attacker_clears_target_after_target_dies() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let attacker = world
            .spawn_unit(PLAYER, GridPosition::new(1, 1))
            .unwrap();

        let target = world
            .spawn_unit(OTHER_PLAYER, GridPosition::new(2, 1))
            .unwrap();

        world
            .handle_command(
                PLAYER,
                ClientCommand::Attack {
                    attackers: vec![attacker],
                    unit: target,
                },
            )
            .unwrap();

        let mut events = Vec::new();


        for _ in 0..10 {
            events.extend(
                world.tick(DEFAULT_UNIT_STATS.base_attack_interval)
            );
        }

        assert_eq!(
            events,
            vec![GameEvent::UnitDied { unit: target }]
        );

        assert!(world.unit(target).is_none());

        assert_eq!(
            world.unit(attacker).unwrap().attack_target,
            None
        );
    }
}
