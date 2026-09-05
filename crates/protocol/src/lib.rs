#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridPosition {
    pub x: i32,
    pub y: i32,
}

impl GridPosition {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldPosition {
    pub x: f32,
    pub y: f32,
}

impl WorldPosition {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientCommand {
    MoveUnits {
        units: Vec<UnitId>,
        destination: GridPosition,
    },
    Attack {
        attackers: Vec<UnitId>,
        unit: UnitId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnitState {
    pub id: UnitId,
    pub owner: PlayerId,
    pub position: WorldPosition,
    pub health: u32,
    pub max_health: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerMessage {
    WorldSnapshot { units: Vec<UnitState> },
    CommandRejected { rejection: CommandRejected },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandRejected {
    EmptyUnitSelection,
    UnitNotFound {
        unit: UnitId,
    },
    NoPath {
        start: GridPosition,
        goal: GridPosition,
    },
    UnitNotOwned {
        unit: UnitId,
    },
    FriendlyTarger {
        unit: UnitId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerId(pub u64);
