mod pathfinding;
pub use pathfinding::find_path;

mod world;
pub use world::{
    AttackError, CommandError, GameEvent, GameWorld, Health, MoveError, Movement, SERVER_TICK, SpawnError,
    Unit, UnitStats, DEFAULT_UNIT_STATS,
};

pub use protocol::{GridPosition, PlayerId, UnitId, WorldPosition};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terrain {
    Grass,
    Water,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    OutsideMap(GridPosition),
}

#[derive(Debug, Clone)]
pub struct GameMap {
    width: i32,
    height: i32,
    terrain: Vec<Terrain>,
}

impl GameMap {
    pub fn new(width: i32, height: i32, fill: Terrain) -> Self {
        assert!(width > 0, "map width must be positive");
        assert!(height > 0, "map height must be positive");

        let tile_count = width
            .checked_mul(height)
            .expect("map dimensions are too large") as usize;

        Self {
            width,
            height,
            terrain: vec![fill; tile_count],
        }
    }

    pub const fn width(&self) -> i32 {
        self.width
    }

    pub const fn height(&self) -> i32 {
        self.height
    }

    pub fn is_inside(&self, position: GridPosition) -> bool {
        position.x >= 0 && position.x < self.width && position.y >= 0 && position.y < self.height
    }

    fn index(&self, position: GridPosition) -> Option<usize> {
        if self.is_inside(position) {
            Some((position.y * self.width + position.x) as usize)
        } else {
            None
        }
    }

    pub fn terrain(&self, position: GridPosition) -> Option<&Terrain> {
        let index = self.index(position)?;
        self.terrain.get(index)
    }

    pub fn set_terrain(
        &mut self,
        position: GridPosition,
        terrain: Terrain,
    ) -> Result<(), MapError> {
        let index = self.index(position).ok_or(MapError::OutsideMap(position))?;

        self.terrain[index] = terrain;

        Ok(())
    }

    pub fn is_walkable(&self, position: GridPosition) -> bool {
        self.terrain(position) == Some(&Terrain::Grass)
    }

    pub fn demo_8x8() -> Self {
        let mut map = Self::new(8, 8, Terrain::Grass);

        for position in [
            GridPosition::new(3, 2),
            GridPosition::new(4, 2),
            GridPosition::new(3, 3),
            GridPosition::new(4, 3),
        ] {
            map.set_terrain(position, Terrain::Water)
                .expect("demo tile must be inside the map");
        }

        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_grid_position() {
        let position = GridPosition::new(4, 7);

        assert_eq!(position.x, 4);
        assert_eq!(position.y, 7);
    }

    #[test]
    fn creates_world_position() {
        let position = WorldPosition::new(4.5, 7.25);

        assert_eq!(position.x, 4.5);
        assert_eq!(position.y, 7.25);
    }

    #[test]
    fn creates_an_8_by_8_grass_map() {
        let map = GameMap::new(8, 8, Terrain::Grass);

        assert_eq!(map.width(), 8);
        assert_eq!(map.height(), 8);
        assert_eq!(map.terrain(GridPosition::new(4, 7)), Some(&Terrain::Grass));
    }

    #[test]
    fn positions_outside_the_map_are_rejected() {
        let map = GameMap::new(8, 8, Terrain::Grass);

        assert!(!map.is_inside(GridPosition::new(-1, 0)));
        assert!(!map.is_inside(GridPosition::new(8, 0)));
        assert!(!map.is_inside(GridPosition::new(0, 8)));
        assert_eq!(map.terrain(GridPosition::new(8, 0)), None);
    }

    #[test]
    fn water_is_not_walkable() {
        let mut map = GameMap::new(8, 8, Terrain::Grass);
        let water_position = GridPosition::new(3, 2);

        map.set_terrain(water_position, Terrain::Water)
            .expect("test position should be inside the map");

        assert!(!map.is_walkable(water_position));
        assert!(map.is_walkable(GridPosition::new(2, 2)));
    }

    #[test]
    fn demo_map_is_8_by_8_and_contains_water() {
        let map = GameMap::demo_8x8();

        assert_eq!(map.width(), 8);
        assert_eq!(map.height(), 8);

        assert_eq!(map.terrain(GridPosition::new(3, 2)), Some(&Terrain::Water));
    }
}
