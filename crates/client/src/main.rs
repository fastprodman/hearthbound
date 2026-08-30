use bevy::prelude::*;
use client::{LocalConnection, ServerConnection};
use game::{GameMap, GameWorld, GridPosition, Terrain};
use protocol::{ServerMessage, UnitId, WorldPosition};
use std::collections::{HashMap, HashSet};

const TILE_WIDTH: f32 = 64.0;
const TILE_HEIGHT: f32 = 32.0;

#[derive(Component)]
struct GameUnitId(UnitId);

#[derive(Resource)]
struct ConnectionResource(LocalConnection);

#[derive(Resource, Default)]
struct RenderedUnitEntities(HashMap<UnitId, Entity>);

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
        .add_systems(Update, (update_connection, sync_server_messages).chain())
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

    world
        .spawn_unit(GridPosition::new(1, 1))
        .expect("demo unit position should be walkable");

    commands.insert_resource(ConnectionResource(LocalConnection::new(world)));
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
