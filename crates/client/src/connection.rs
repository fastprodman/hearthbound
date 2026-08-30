use std::{collections::VecDeque, time::Duration};

use game::{CommandError, GameWorld, MoveError, SERVER_TICK};
use protocol::{ClientCommand, CommandRejected, ServerMessage};

pub trait ServerConnection {
    fn send(&mut self, command: ClientCommand);
    fn update(&mut self, delta: Duration);
    fn receive(&mut self) -> Vec<ServerMessage>;
}

pub struct LocalConnection {
    world: GameWorld,
    incoming: VecDeque<ClientCommand>,
    outgoing: VecDeque<ServerMessage>,
    accumulator: Duration,
}

impl LocalConnection {
    pub fn new(world: GameWorld) -> Self {
        let initial_snapshot = world.snapshot();

        Self {
            world,
            incoming: VecDeque::new(),
            outgoing: VecDeque::from([initial_snapshot]),
            accumulator: Duration::ZERO,
        }
    }

    fn run_server_tick(&mut self) {
        while let Some(command) = self.incoming.pop_front() {
            if let Err(error) = self.world.handle_command(command) {
                self.outgoing.push_back(ServerMessage::CommandRejected {
                    rejection: command_rejection(error),
                });
            }
        }

        self.world.tick(SERVER_TICK);
        self.outgoing.push_back(self.world.snapshot());
    }
}

impl ServerConnection for LocalConnection {
    fn send(&mut self, command: ClientCommand) {
        self.incoming.push_back(command);
    }

    fn update(&mut self, delta: Duration) {
        self.accumulator += delta;

        while self.accumulator >= SERVER_TICK {
            self.run_server_tick();
            self.accumulator -= SERVER_TICK;
        }
    }

    fn receive(&mut self) -> Vec<ServerMessage> {
        self.outgoing.drain(..).collect()
    }
}

fn command_rejection(error: CommandError) -> CommandRejected {
    match error {
        CommandError::EmptyUnitSelection => CommandRejected::EmptyUnitSelection,
        CommandError::Move(MoveError::UnitNotFound(unit)) => CommandRejected::UnitNotFound { unit },
        CommandError::Move(MoveError::NoPath { start, goal }) => {
            CommandRejected::NoPath { start, goal }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game::{GameMap, GridPosition, Terrain};
    use protocol::{ServerMessage, UnitState, WorldPosition};

    fn world_with_one_unit() -> (GameWorld, protocol::UnitId) {
        let map = GameMap::new(8, 8, Terrain::Grass);
        let mut world = GameWorld::new(map);

        let id = world.spawn_unit(GridPosition::new(0, 0)).unwrap();

        (world, id)
    }

    #[test]
    fn provides_an_initial_snapshot() {
        let (world, id) = world_with_one_unit();
        let mut connection = LocalConnection::new(world);

        assert_eq!(
            connection.receive(),
            vec![ServerMessage::WorldSnapshot {
                units: vec![UnitState {
                    id,
                    position: WorldPosition::new(0.0, 0.0),
                }],
            }]
        );
    }

    #[test]
    fn sending_a_command_does_not_update_immediately() {
        let (world, id) = world_with_one_unit();
        let mut connection = LocalConnection::new(world);

        // Remove the initial snapshot.
        connection.receive();

        connection.send(ClientCommand::MoveUnits {
            units: vec![id],
            destination: GridPosition::new(2, 0),
        });

        assert!(connection.receive().is_empty());

        connection.update(Duration::from_millis(25));
        assert!(connection.receive().is_empty());

        connection.update(Duration::from_millis(25));

        let messages = connection.receive();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn local_connection_moves_unit_using_fixed_ticks() {
        let (world, id) = world_with_one_unit();
        let mut connection = LocalConnection::new(world);

        connection.receive();

        connection.send(ClientCommand::MoveUnits {
            units: vec![id],
            destination: GridPosition::new(2, 0),
        });

        connection.update(Duration::from_secs(2));

        let messages = connection.receive();
        let final_message = messages.last().expect("server should produce snapshots");

        assert_eq!(
            final_message,
            &ServerMessage::WorldSnapshot {
                units: vec![UnitState {
                    id,
                    position: WorldPosition::new(2.0, 0.0),
                }],
            }
        );
    }

    #[test]
    fn rejected_command_is_reported_to_client() {
        let (world, _) = world_with_one_unit();
        let mut connection = LocalConnection::new(world);

        // Discard the initial snapshot.
        connection.receive();

        connection.send(ClientCommand::MoveUnits {
            units: vec![],
            destination: GridPosition::new(2, 2),
        });

        connection.update(SERVER_TICK);

        let messages = connection.receive();

        assert_eq!(messages.len(), 2);

        assert_eq!(
            messages.first(),
            Some(&ServerMessage::CommandRejected {
                rejection: CommandRejected::EmptyUnitSelection,
            })
        );

        assert!(matches!(
            messages.get(1),
            Some(ServerMessage::WorldSnapshot { .. })
        ));
    }
}
