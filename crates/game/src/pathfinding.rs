use std::collections::{HashMap, VecDeque};

use crate::{GameMap, GridPosition};

pub fn find_path(
    map: &GameMap,
    start: GridPosition,
    goal: GridPosition,
) -> Option<VecDeque<GridPosition>> {
    if !map.is_walkable(start) || !map.is_walkable(goal) {
        return None;
    }

    let mut frontier = VecDeque::from([start]);
    let mut came_from = HashMap::from([(start, None)]);

    while let Some(current) = frontier.pop_front() {
        if current == goal {
            return reconstruct_path(&came_from, start, goal);
        }

        for neighbor in cardinal_neighbors(current) {
            if map.is_walkable(neighbor) && !came_from.contains_key(&neighbor) {
                frontier.push_back(neighbor);
                came_from.insert(neighbor, Some(current));
            }
        }
    }

    None
}

fn cardinal_neighbors(position: GridPosition) -> [GridPosition; 4] {
    [
        GridPosition::new(position.x + 1, position.y),
        GridPosition::new(position.x - 1, position.y),
        GridPosition::new(position.x, position.y + 1),
        GridPosition::new(position.x, position.y - 1),
    ]
}

fn reconstruct_path(
    came_from: &HashMap<GridPosition, Option<GridPosition>>,
    start: GridPosition,
    goal: GridPosition,
) -> Option<VecDeque<GridPosition>> {
    let mut path = VecDeque::new();
    let mut current = goal;

    while current != start {
        path.push_front(current);
        current = came_from.get(&current).copied().flatten()?;
    }

    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Terrain;

    #[test]
    fn finds_a_shortest_path() {
        let map = GameMap::new(4, 4, Terrain::Grass);

        let path = find_path(&map, GridPosition::new(0, 0), GridPosition::new(2, 1))
            .expect("a path should exist");

        assert_eq!(path.len(), 3);
        assert_eq!(path.back(), Some(&GridPosition::new(2, 1)));
    }

    #[test]
    fn path_avoids_water() {
        let mut map = GameMap::new(3, 3, Terrain::Grass);

        map.set_terrain(GridPosition::new(1, 0), Terrain::Water)
            .unwrap();

        let path = find_path(&map, GridPosition::new(0, 0), GridPosition::new(2, 0))
            .expect("the unit should walk around the water");

        assert_eq!(path.len(), 4);
        assert!(!path.contains(&GridPosition::new(1, 0)));
    }

    #[test]
    fn water_target_has_no_path() {
        let mut map = GameMap::new(3, 3, Terrain::Grass);
        let goal = GridPosition::new(2, 2);

        map.set_terrain(goal, Terrain::Water).unwrap();

        assert_eq!(find_path(&map, GridPosition::new(0, 0), goal), None);
    }

    #[test]
    fn start_equal_to_goal_returns_empty_path() {
        let map = GameMap::new(3, 3, Terrain::Grass);
        let position = GridPosition::new(1, 1);

        assert_eq!(find_path(&map, position, position), Some(VecDeque::new()));
    }

    #[test]
    fn outside_destination_has_no_path() {
        let map = GameMap::new(3, 3, Terrain::Grass);

        assert_eq!(
            find_path(&map, GridPosition::new(0, 0), GridPosition::new(3, 0),),
            None
        );
    }
}
