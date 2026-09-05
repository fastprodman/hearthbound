use bevy::prelude::*;
use client::{LocalConnection, ServerConnection};
use game::{GameMap, GameWorld, GridPosition, PlayerId, Terrain};
use protocol::{ClientCommand, ServerMessage, UnitId, WorldPosition};
use std::collections::{HashMap, HashSet};

const TILE_WIDTH: f32 = 64.0;
const TILE_HEIGHT: f32 = 32.0;
const UNIT_PICK_RADIUS: f32 = 16.0;

const LOCAL_PLAYER: PlayerId = PlayerId(1);

#[derive(Component)]
struct GameUnitId(UnitId);

#[derive(Resource)]
struct ConnectionResource(LocalConnection);

#[derive(Resource, Default)]
struct RenderedUnitEntities(HashMap<UnitId, Entity>);

#[derive(Resource, Default)]
struct SelectedUnits(HashSet<UnitId>);

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.11)))
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Hearthbound".into(),
                        resolution: (1280, 720).into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .init_resource::<RenderedUnitEntities>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                select_unit,
                send_move_command,
                update_connection,
                sync_server_messages,
                draw_seleciton,
            )
                .chain(),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn(Camera2d);

    let map = GameMap::demo_8x8();

    spawn_map(&mut commands, &mut meshes, &mut materials, &map);

    let mut world = GameWorld::new(map);

    let first_unit = world
        .spawn_unit(LOCAL_PLAYER, GridPosition::new(1, 1))
        .expect("demo unit position should be walkable");

    world
        .spawn_unit(LOCAL_PLAYER, GridPosition::new(2, 1))
        .expect("demo unit position should be walkable");

    world
        .spawn_unit(LOCAL_PLAYER, GridPosition::new(1, 2))
        .expect("demo unit position should be walkable");

    commands.insert_resource(SelectedUnits(HashSet::from([first_unit])));

    commands.insert_resource(ConnectionResource(LocalConnection::new(
        LOCAL_PLAYER,
        world,
    )));
}

fn update_connection(time: Res<Time>, mut connection: ResMut<ConnectionResource>) {
    connection.0.update(time.delta());
}

fn sync_server_messages(
    mut commands: Commands,
    mut connection: ResMut<ConnectionResource>,
    mut rendered_unit_entities: ResMut<RenderedUnitEntities>,
    mut rendered_units: Query<(&GameUnitId, &mut Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut latest_units = None;

    for message in connection.0.receive() {
        match message {
            ServerMessage::WorldSnapshot { units } => {
                latest_units = Some(units);
            }

            ServerMessage::CommandRejected { rejection } => {
                warn!("server rejected command: {rejection:?}");
            }
        }
    }

    let Some(unit_states) = latest_units else {
        return;
    };

    let visible_unit_ids: HashSet<UnitId> = unit_states.iter().map(|state| state.id).collect();

    for state in unit_states {
        // Indicates whether unit already exists in render.
        let existing_entity = rendered_unit_entities.0.get(&state.id).copied();

        if let Some(entity) = existing_entity {
            match rendered_units.get_mut(entity) {
                Ok((game_unit_id, mut transform)) => {
                    debug_assert_eq!(game_unit_id.0, state.id);

                    transform.translation = unit_translation(state.position);
                    continue;
                }

                Err(error) => {
                    warn!(
                        ?error,
                        ?entity,
                        unit_id = state.id.0,
                        "removing stale rendered-unit mapping"
                    );

                    rendered_unit_entities.0.remove(&state.id);
                }
            }
        }

        let entity = commands
            .spawn((
                GameUnitId(state.id),
                Mesh2d(meshes.add(Circle::new(10.0))),
                MeshMaterial2d(materials.add(Color::srgb(0.92, 0.72, 0.24))),
                unit_transform(state.position),
            ))
            .id();

        rendered_unit_entities.0.insert(state.id, entity);
    }

    let removed_unit_ids: Vec<UnitId> = rendered_unit_entities
        .0
        .keys()
        .filter(|rendered_unit_id| !visible_unit_ids.contains(rendered_unit_id))
        .copied()
        .collect();

    for id in removed_unit_ids {
        if let Some(entity) = rendered_unit_entities.0.remove(&id) {
            commands.entity(entity).despawn();
        }
    }
}

fn send_move_command(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    selected_units: Res<SelectedUnits>,
    mut connection: ResMut<ConnectionResource>,
) {
    if !mouse_buttons.just_pressed(MouseButton::Right) {
        return;
    }

    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    let (camera, camera_transform) = camera.into_inner();

    let Ok(render_position) = camera.viewport_to_world_2d(camera_transform, cursor_position) else {
        return;
    };

    let destination = bevy_to_grid(render_position);

    let mut units: Vec<UnitId> = selected_units.0.iter().copied().collect();

    if units.is_empty() {
        return;
    }

    units.sort_by_key(|id| id.0);

    connection
        .0
        .send(ClientCommand::MoveUnits { units, destination });
}

fn select_unit(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut selected_units: ResMut<SelectedUnits>,
    rendered_units: Query<(&GameUnitId, &GlobalTransform)>,
) {
    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }

    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    let (camera, camera_transform) = camera.into_inner();

    let Ok(cursor_world) = camera.viewport_to_world_2d(camera_transform, cursor_position) else {
        return;
    };

    let mut nearest_unit: Option<(UnitId, f32)> = None;

    for (game_unit_id, transform) in &rendered_units {
        let unit_position = transform.translation().truncate();

        let distance_squared = cursor_world.distance_squared(unit_position);

        if distance_squared > UNIT_PICK_RADIUS.powi(2) {
            continue;
        }

        let is_nearest =
            nearest_unit.is_none_or(|(_, best_distance)| distance_squared < best_distance);

        if is_nearest {
            nearest_unit = Some((game_unit_id.0, distance_squared));
        }
    }

    selected_units.0.clear();

    if let Some((unit_id, _)) = nearest_unit {
        selected_units.0.insert(unit_id);
    }
}

fn draw_seleciton(
    selected_units: Res<SelectedUnits>,
    rendered_units: Query<(&GameUnitId, &GlobalTransform)>,
    mut gizmos: Gizmos,
) {
    for (game_unit_id, transform) in &rendered_units {
        if !selected_units.0.contains(&game_unit_id.0) {
            continue;
        }

        let position = transform.translation().truncate();

        gizmos.circle_2d(position, UNIT_PICK_RADIUS, Color::srgb(1.0, 0.9, 0.25));
    }
}

fn unit_translation(position: WorldPosition) -> Vec3 {
    let render_position = world_to_bevy(position);
    let depth = 1.0 + (position.x + position.y) * 0.001;

    Vec3::new(render_position.x, render_position.y, depth)
}

fn unit_transform(position: WorldPosition) -> Transform {
    Transform::from_translation(unit_translation(position))
}

fn spawn_map(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    map: &GameMap,
) {
    let tile_mesh = meshes.add(Rhombus::new(TILE_WIDTH, TILE_HEIGHT));

    let grass_material = materials.add(Color::srgb(0.32, 0.58, 0.30));

    let water_material = materials.add(Color::srgb(0.18, 0.40, 0.68));

    for y in 0..map.height() {
        for x in 0..map.width() {
            let position = GridPosition::new(x, y);

            let material = match map.terrain(position) {
                Some(&Terrain::Grass) => grass_material.clone(),
                Some(&Terrain::Water) => water_material.clone(),
                None => continue,
            };

            let render_position = grid_to_bevy(position);
            let depth = (x + y) as f32 * 0.001;

            commands.spawn((
                Mesh2d(tile_mesh.clone()),
                MeshMaterial2d(material),
                Transform::from_xyz(render_position.x, render_position.y, depth),
            ));
        }
    }
}

fn grid_to_bevy(position: GridPosition) -> Vec2 {
    world_to_bevy(WorldPosition::new(position.x as f32, position.y as f32))
}

fn world_to_bevy(position: WorldPosition) -> Vec2 {
    Vec2::new(
        (position.x - position.y) * TILE_WIDTH * 0.5,
        -(position.x + position.y) * TILE_HEIGHT * 0.5,
    )
}

fn bevy_to_grid(position: Vec2) -> GridPosition {
    let logical_x = position.x / TILE_WIDTH - position.y / TILE_HEIGHT;
    let logical_y = -position.x / TILE_WIDTH - position.y / TILE_HEIGHT;

    GridPosition::new(logical_x.round() as i32, logical_y.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_position_survives_render_round_trip() {
        for y in 0..8 {
            for x in 0..8 {
                let original = GridPosition::new(x, y);
                let rendered = grid_to_bevy(original);
                let converted = bevy_to_grid(rendered);

                assert_eq!(converted, original);
            }
        }
    }
}
