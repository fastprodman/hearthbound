use crate::{GameMap, GridPosition, UnitId, WorldPosition, find_path};
use protocol::ClientCommand;
use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

pub const SERVER_TICK: Duration = Duration::from_millis(50);
const UNIT_SPEED: f32 = 2.0;

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
}

impl From<MoveError> for CommandError {
    fn from(error: MoveError) -> Self {
        Self::Move(error)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Movement {
    pub path: VecDeque<GridPosition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub id: UnitId,
    pub position: WorldPosition,
    pub movement: Option<Movement>,
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

    pub fn spawn_unit(&mut self, position: GridPosition) -> Result<UnitId, SpawnError> {
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
            position: WorldPosition::new(position.x as f32, position.y as f32),
            movement: None,
        };

        self.units.insert(id, unit);

        Ok(id)
    }

    pub fn handle_command(&mut self, commnad: ClientCommand) -> Result<(), CommandError> {
        match commnad {
            ClientCommand::MoveUnits { units, destination } => {
                if units.is_empty() {
                    return Err(CommandError::EmptyUnitSelection);
                }

                // Validate and calculate every path before modifying units.
                let mut planned_movements = Vec::with_capacity(units.len());

                for id in units {
                    let path = self.plan_move(id, destination)?;
                    planned_movements.push((id, path));
                }

                // All movement requests are valid, so they can now be applied.
                for (id, path) in planned_movements {
                    self.apply_movement(id, path);
                }

                Ok(())
            }
        }
    }

    fn plan_move(
        &self,
        id: UnitId,
        goal: GridPosition,
    ) -> Result<VecDeque<GridPosition>, MoveError> {
        let unit = self.units.get(&id).ok_or(MoveError::UnitNotFound(id))?;

        let start = GridPosition::new(
            unit.position.x.round() as i32,
            unit.position.y.round() as i32,
        );

        find_path(&self.map, start, goal).ok_or(MoveError::NoPath { start, goal })
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

    #[test]
    fn spawns_a_unit_on_walkable_terrain() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let id = world
            .spawn_unit(GridPosition::new(2, 3))
            .expect("spawn position should be walkable");

        let unit = world.unit(id).expect("spawned unit should exist");

        assert_eq!(unit.id, id);
        assert_eq!(unit.position, WorldPosition::new(2.0, 3.0));
    }

    #[test]
    fn assigns_unique_unit_ids() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let first = world.spawn_unit(GridPosition::new(1, 1)).unwrap();
        let second = world.spawn_unit(GridPosition::new(2, 1)).unwrap();

        assert_ne!(first, second);
    }

    #[test]
    fn rejects_a_unit_spawned_on_water() {
        let mut map = GameMap::new(8, 8, Terrain::Grass);
        let water = GridPosition::new(3, 2);

        map.set_terrain(water, Terrain::Water).unwrap();

        let mut world = GameWorld::new(map);

        assert_eq!(
            world.spawn_unit(water),
            Err(SpawnError::PositionNotWalkable(water))
        );
    }

    #[test]
    fn unit_reaches_its_destination() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let id = world.spawn_unit(GridPosition::new(0, 0)).unwrap();

        world
            .handle_command(ClientCommand::MoveUnits {
                units: vec![id],
                destination: GridPosition::new(4, 3),
            })
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
        let id = world.spawn_unit(GridPosition::new(0, 0)).unwrap();

        assert_eq!(
            world.handle_command(ClientCommand::MoveUnits {
                units: vec![id],
                destination: water,
            }),
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
            world.handle_command(ClientCommand::MoveUnits {
                units: vec![],
                destination: GridPosition::new(2, 2),
            }),
            Err(CommandError::EmptyUnitSelection)
        );
    }

    #[test]
    fn invalid_group_command_does_not_move_valid_unit() {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let valid_id = world.spawn_unit(GridPosition::new(0, 0)).unwrap();

        let missing_id = UnitId(999);

        assert_eq!(
            world.handle_command(ClientCommand::MoveUnits {
                units: vec![valid_id, missing_id],
                destination: GridPosition::new(4, 3),
            }),
            Err(CommandError::Move(MoveError::UnitNotFound(missing_id)))
        );

        assert_eq!(world.unit(valid_id).unwrap().movement, None);
    }
}
