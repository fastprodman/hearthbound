use crate::{GameMap, GridPosition, UnitId, WorldPosition, find_path};
use protocol::{ClientCommand, PlayerId, ServerMessage, UnitState};
use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

pub const SERVER_TICK: Duration = Duration::from_millis(50);
const UNIT_SPEED: f32 = 2.0;
pub const UNIT_MAX_HEALTH: u32 = 100;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    PositionNotWalkable(GridPosition),
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
            health: Health::new(UNIT_MAX_HEALTH),
            attack_target: None,
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
            ClientCommand::Attack { attackers, unit } => self.handle_attack(owner, attackers, unit),
        }
    }

    fn handle_attack(
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
    }

    pub fn tick(&mut self, delta: Duration) {
        for unit in self.units.values_mut() {
            tick_unit(unit, delta);
        }
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
    let maximum_step = UNIT_SPEED * delta.as_secs_f32();

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
                        health: UNIT_MAX_HEALTH,
                        max_health: UNIT_MAX_HEALTH,
                    },
                    UnitState {
                        id: second,
                        owner: PLAYER,
                        position: WorldPosition::new(4.0, 5.0),
                        health: UNIT_MAX_HEALTH,
                        max_health: UNIT_MAX_HEALTH,
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

        assert_eq!(unit.health.current(), UNIT_MAX_HEALTH);
        assert_eq!(unit.health.maximum(), UNIT_MAX_HEALTH);
        assert!(!unit.health.is_dead());
    }

    #[test]
    fn damage_cannot_reduce_health_below_zero() {
        let mut health = Health::new(100);

        health.apply_damage(150);

        assert_eq!(health.current(), 0);
        assert!(health.is_dead());
    }
}
