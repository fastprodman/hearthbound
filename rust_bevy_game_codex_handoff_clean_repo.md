# Rust + Bevy Multiplayer Game — Codex Handoff Specification

## Purpose

This document defines how I want to build my game from a **new clean repository from scratch**.

The old learning repository:

https://github.com/fastprodman/beavy-learn-1

should be treated only as a **reference / learning prototype**.

Do **not** refactor that repository into the new architecture.

Instead:

1. Create a new clean Rust workspace.
2. Build the real architecture from the beginning.
3. Reuse ideas and small algorithms from the old repo only when helpful.
4. Keep each implementation step small and understandable.
5. Design the game so the local embedded server can later be replaced by a real network server with minimal client-side changes.

I am:

- A senior Go developer.
- Learning Rust.
- New to game development.
- Using Bevy for the client.
- Intending to build a server-authoritative multiplayer game.

When helping me, explain important Rust, Bevy, ECS, game-loop, and architecture concepts instead of only generating code.

---

# 1. Game concept

The game is a **2D isometric multiplayer extraction / strategy game**.

Visual direction:

- 2D pixel art.
- Isometric.
- Simple and cozy.
- Art direction inspired by ISOCORE.

Core loop:

```text
Prepare at safe base
        ↓
Enter persistent island
        ↓
Gather / fight / contest
        ↓
Attempt extraction
        ↓
Return with loot
        ↓
Improve base and army
        ↓
Repeat
```

---

# 2. Offline base

Each player has a private safe offline base.

The base is persistent.

Players can:

- Build unit spawners.
- Create troops.
- Organize troops.
- Build farms.
- Produce food.
- Construct buildings.
- Upgrade buildings.
- Craft items.
- Prepare troops for island expeditions.

Players cannot be attacked at their offline base.

Players should be able to safely log off there.

---

# 3. Islands

Islands are persistent shared online maps.

They contain:

- Trees.
- Ore.
- Other gathering resources.
- PvE animals that provide food.
- PvE monsters that provide loot.
- High-value resource locations.
- Buildings constructed by players/clans.
- PvP between players.
- Clan competition.

Islands remain active independently of any single player client.

This means island simulation must eventually run headlessly on a server.

---

# 4. Extraction / Return mechanic

Players cannot instantly teleport back to safety.

To leave an island:

1. Player activates a Return Spell.
2. A Return Beacon appears.
3. The beacon becomes visible to nearby players.
4. A countdown starts.
5. Other players may attack or contest the extraction.
6. The player must survive until the countdown finishes.
7. Surviving troops and collected resources return to the offline base.

This mechanic is central to the game.

---

# 5. Important architectural principle

The game must be designed as:

```text
authoritative game simulation
            +
Bevy graphical client
```

The game simulation must **not depend on Bevy**.

Bevy should primarily handle:

- Rendering.
- Mouse and keyboard input.
- Camera.
- UI.
- Animations.
- Visual effects.
- Selection indicators.
- Client interpolation/prediction later.

The authoritative game simulation should handle:

- Maps.
- Units.
- Positions.
- Movement.
- Pathfinding.
- Combat.
- Resource gathering.
- Buildings.
- Ownership.
- Extraction.
- Timers.
- AI.
- Validation.
- Server rules.

---

# 6. New repository — do not refactor the old prototype

Create a new repository/workspace.

The old `beavy-learn-1` repository is useful only for reference.

It already demonstrated:

- Isometric rendering.
- Grid ↔ Bevy coordinate conversion.
- Click-to-move.
- BFS pathfinding.
- Obstacle avoidance.
- Path replanning.
- Isometric depth sorting.

Those ideas can be reused selectively.

However, do not copy its architecture because the old prototype stores movement state directly in Bevy components and `Transform`.

The new project should begin with the correct separation.

---

# 7. Initial workspace structure

Start with this workspace:

```text
game-project/
│
├── Cargo.toml
│
└── crates/
    ├── game/
    │   ├── Cargo.toml
    │   └── src/
    │       └── lib.rs
    │
    ├── protocol/
    │   ├── Cargo.toml
    │   └── src/
    │       └── lib.rs
    │
    ├── client/
    │   ├── Cargo.toml
    │   └── src/
    │       └── main.rs
    │
    └── server/
        ├── Cargo.toml
        └── src/
            └── main.rs
```

Do not immediately create many source files.

Split files only when a module becomes large enough to justify it.

Potential future structure:

```text
crates/game/src/
├── lib.rs
├── position.rs
├── map.rs
├── unit.rs
├── movement.rs
├── pathfinding.rs
└── world.rs
```

and:

```text
crates/client/src/
├── main.rs
├── input.rs
├── rendering.rs
├── sync.rs
└── connection/
    ├── mod.rs
    ├── local.rs
    └── network.rs
```

But do not create empty abstraction-heavy modules prematurely.

---

# 8. Workspace dependency rules

Critical rule:

```text
game       → NO Bevy
protocol   → NO Bevy
server     → NO Bevy
client     → Bevy
```

Long-term dependency direction:

```text
             protocol
             ▲      ▲
             │      │
          client  server
                    │
                    ▼
                   game
```

For local development, the client may additionally depend on `game` so it can host an embedded local server implementation.

Later, when using a real server, the normal client path should not depend on direct access to authoritative `GameWorld`.

---

# 9. Core coordinates

Do not use Bevy coordinate types in `game` or `protocol`.

Do not use:

```rust
IVec2
Vec2
Transform
Entity
```

as authoritative game types.

Create our own types.

## GridPosition

Used for:

- Tiles.
- Pathfinding.
- Buildings.
- Resource nodes.
- Map occupancy.

Example:

```rust
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
```

## WorldPosition

Used for continuous authoritative simulation:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldPosition {
    pub x: f32,
    pub y: f32,
}
```

Recommended convention:

```text
1 tile = 1.0 logical world unit
```

Examples:

```text
Tile (4, 7)
center ≈ WorldPosition(4.0, 7.0)

Moving unit:
WorldPosition(4.37, 7.0)
```

Use `WorldPosition` for:

- Movement.
- Attack distance.
- AoE range.
- Collision/radius checks later.

---

# 10. Bevy coordinates are rendering only

The Bevy client converts logical positions into isometric render positions.

Example:

```text
WorldPosition(4.2, 7.6)
       ↓
isometric projection
       ↓
Bevy Vec2
       ↓
Transform
       ↓
screen
```

The server never needs to know the rendered isometric X/Y.

Distance calculations must happen in logical game coordinates, not Bevy screen/world coordinates.

---

# 11. Map storage

Start with a dense rectangular map.

Example:

```rust
pub struct GameMap {
    width: i32,
    height: i32,
    terrain: Vec<Terrain>,
    blocked: HashSet<GridPosition>,
}
```

A dense rectangular `Vec` is preferred over:

```rust
HashMap<GridPosition, Tile>
```

for the initial prototype.

Index concept:

```text
index = y * width + x
```

---

# 12. Terrain layer

Initial terrain:

```rust
pub enum Terrain {
    Grass,
    Water,
}
```

Later possibly:

```rust
Sand
Rock
Mud
Road
```

Terrain may eventually answer:

- Is this tile walkable?
- Is it buildable?
- What is movement cost?

Do not build a large metadata system yet.

---

# 13. Logical map layers

Start with three conceptual layers.

## Layer 1 — Terrain

Dense grid.

Examples:

```text
G G G G G
G G W W G
G G W W G
G G G G G
```

## Layer 2 — Static occupancy

Examples:

- Trees.
- Ore.
- Buildings.
- Walls.

These objects live in `GameWorld`.

The map keeps a fast index indicating whether a position is blocked.

Initially:

```rust
HashSet<GridPosition>
```

Later this might become:

```rust
HashMap<GridPosition, ObjectId>
```

if we need to know what occupies a tile.

## Layer 3 — Dynamic entities

Examples:

- Units.
- Monsters.
- Players.
- Projectiles.

Do not store them directly inside tiles.

Example:

```rust
pub struct GameWorld {
    pub map: GameMap,
    pub units: HashMap<UnitId, Unit>,
}
```

Later:

```rust
resources
buildings
monsters
```

can be added.

---

# 14. Map API

Initial map should expose simple operations such as:

```rust
impl GameMap {
    pub fn is_inside(&self, pos: GridPosition) -> bool;

    pub fn terrain(&self, pos: GridPosition) -> Option<&Terrain>;

    pub fn is_walkable(&self, pos: GridPosition) -> bool;

    pub fn block(&mut self, pos: GridPosition);

    pub fn unblock(&mut self, pos: GridPosition);
}
```

Pathfinding should only need:

```rust
map.is_walkable(position)
```

Pathfinding should not care whether something is blocked because of:

- Water.
- Tree.
- Rock.
- Wall.
- Building.

---

# 15. Resource nodes

Do not embed full trees/resources directly inside tiles.

Represent them separately.

Example:

```rust
pub struct ResourceNode {
    pub id: ResourceNodeId,
    pub position: GridPosition,
    pub kind: ResourceKind,
    pub remaining: u32,
}
```

When a tree exists:

```text
GameWorld.resources contains tree
+
GameMap marks the tile blocked
```

When it is removed:

```text
remove resource
+
unblock tile
```

---

# 16. Map definition vs live runtime state

Eventually distinguish between:

```text
MapDefinition
```

and:

```text
GameWorld runtime state
```

Example definition:

```rust
pub struct MapDefinition {
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<Tile>,
    pub resource_spawns: Vec<ResourceSpawn>,
    pub monster_spawns: Vec<MonsterSpawn>,
}
```

Definition:

```text
tree spawn at (10, 12), maximum 100 wood
```

Runtime:

```text
tree #42 at (10, 12), remaining 37 wood
```

Do not implement this distinction too early unless needed by a milestone.

---

# 17. Pathfinding

Authoritative pathfinding belongs to `game`.

It must not depend on Bevy.

Target API:

```rust
pub fn find_path(
    map: &GameMap,
    start: GridPosition,
    goal: GridPosition,
) -> Option<VecDeque<GridPosition>>
```

For the first implementation, reuse/adapt the BFS logic from the learning prototype.

Pathfinding should:

- Handle map bounds.
- Avoid blocked tiles.
- Support current diagonal movement rules if desired.
- Have unit tests.
- Not know about Bevy.
- Not know about `TREE_GRID`.
- Not know about sprites.

Later it may become A* if necessary.

Do not optimize prematurely.

---

# 18. GameWorld

Create an authoritative world model.

Initial version:

```rust
pub struct GameWorld {
    pub map: GameMap,
    pub units: HashMap<UnitId, Unit>,
}
```

IDs should be our own stable game IDs.

Example:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitId(pub u64);
```

Do not use Bevy `Entity` as an authoritative ID.

---

# 19. Unit model

Initial server-side unit:

```rust
pub struct Unit {
    pub id: UnitId,
    pub position: WorldPosition,
    pub movement: Option<Movement>,
}
```

Initial movement:

```rust
pub struct Movement {
    pub path: VecDeque<GridPosition>,
}
```

The server should store the real position in logical `WorldPosition`.

The path may remain tile based.

---

# 20. Headless movement before graphics

This is a critical difference from the old prototype.

Before implementing Bevy movement, first prove that one unit can move in the **headless game simulation**.

Example test/use:

```rust
let mut world = GameWorld::new(...);

world.spawn_unit(...);

world.issue_move(...);

for _ in 0..100 {
    world.tick(Duration::from_millis(50));
}
```

Then assert that the unit reached its target.

The first authoritative movement implementation must not use:

```rust
Transform
Time
Vec2
Sprite
Camera
```

---

# 21. Fixed simulation tick

The authoritative simulation should use a fixed tick.

Early target:

```text
20–30 server ticks / second
```

Rendering FPS can remain independent:

```text
60 / 120 / 144 / variable
```

Local mode should also simulate using fixed ticks.

Concept:

```rust
accumulator += frame_delta;

while accumulator >= SERVER_TICK {
    game_world.tick(SERVER_TICK);
    accumulator -= SERVER_TICK;
}
```

This makes local mode behave closer to the eventual real server.

---

# 22. Core simulation should stay synchronous

Do not make `GameWorld` async.

Preferred style:

```rust
impl GameWorld {
    pub fn handle_command(&mut self, command: ClientCommand) {
        // Validate.
        // Update authoritative intent/state.
    }

    pub fn tick(&mut self, dt: Duration) {
        // Movement.
        // Combat.
        // Gathering.
        // AI.
        // Extraction timers.
    }
}
```

Do not use Tokio inside:

- Pathfinding.
- Movement.
- Combat.
- Gathering.
- Map logic.

Networking may use async later, but the simulation core should remain synchronous.

---

# 23. Protocol crate

The protocol defines communication between client and server.

It must not depend on Bevy.

Start with a movement command.

Example:

```rust
pub enum ClientCommand {
    MoveUnits {
        units: Vec<UnitId>,
        destination: GridPosition,
    },
}
```

Later:

```rust
Attack
Gather
StartExtraction
Build
```

The key rule:

```text
Client sends intent.
Server decides reality.
```

Do not design protocol messages such as:

```text
"My unit is now at X/Y."
"I killed this enemy."
"I gained 100 wood."
```

Instead:

```text
"Move these units there."
"Attack this target."
"Gather from this node."
```

---

# 24. Server messages

Initial message design can be simple.

Example:

```rust
pub enum ServerMessage {
    UnitSpawned {
        id: UnitId,
        position: WorldPosition,
    },

    UnitMoved {
        id: UnitId,
        position: WorldPosition,
    },

    UnitDied {
        id: UnitId,
    },
}
```

This may later evolve into:

- Snapshots.
- Deltas.
- Tick-based state replication.

Do not overdesign the final replication protocol now.

---

# 25. Local server abstraction

This is one of the main design goals.

The Bevy client should not directly manipulate `GameWorld`.

Even during local development, put a server-like boundary between them.

Concept:

```text
Bevy Client
     |
     | ClientCommand
     v
ServerConnection
     |
     v
LocalConnection
     |
     v
GameWorld
```

Later:

```text
Bevy Client
     |
     | ClientCommand
     v
ServerConnection
     |
     v
NetworkConnection
     |
     v
real network
     |
     v
server process
     |
     v
GameWorld
```

---

# 26. Avoid synchronous RPC semantics

Do not design:

```rust
let response = server.move_unit(...);
```

That assumes an immediate response.

Prefer:

```rust
connection.send(command);
```

and separately:

```rust
for message in connection.receive() {
    // update client
}
```

Even the local implementation should behave this way.

---

# 27. ServerConnection

Conceptual interface:

```rust
pub trait ServerConnection {
    fn send(&mut self, command: ClientCommand);

    fn receive(&mut self) -> Vec<ServerMessage>;

    fn update(&mut self, dt: Duration);
}
```

The exact Rust API can change if a cleaner design works better with Bevy resources.

The important requirement is semantic:

- Client submits commands.
- Server progresses independently.
- Client receives updates later.
- No direct authoritative game mutation from Bevy input systems.

---

# 28. LocalConnection

Initial implementation may own:

```rust
pub struct LocalConnection {
    world: GameWorld,
    incoming: VecDeque<ClientCommand>,
    outgoing: VecDeque<ServerMessage>,
}
```

No Tokio required.

No threads required.

No network required.

Typical update:

```text
Bevy frame
    |
    +-> player input
    |
    +-> send ClientCommand
    |
    +-> LocalConnection queues it
    |
    +-> LocalConnection runs fixed server ticks
    |
    +-> GameWorld handles commands
    |
    +-> GameWorld updates simulation
    |
    +-> outgoing ServerMessages created
    |
    +-> Bevy receives updates
```

---

# 29. NetworkConnection later

Eventually add:

```rust
NetworkConnection
```

It may internally use:

- Tokio.
- Channels.
- QUIC / WebSocket / another transport.

But the Bevy gameplay/input systems should not need major changes.

Goal:

Today:

```text
client → LocalConnection → GameWorld
```

Later:

```text
client → NetworkConnection → network → server → GameWorld
```

The high-level client code should keep using the same command/message boundary.

---

# 30. Bevy client responsibilities

Only add Bevy after the headless simulation works.

Bevy client should:

- Render map terrain.
- Convert logical coordinates to isometric coordinates.
- Spawn sprites for server-owned units.
- Handle camera.
- Handle input.
- Show selection.
- Translate user actions into `ClientCommand`.
- Receive server state.
- Update rendering.

Bevy must not be authoritative for gameplay.

---

# 31. Isometric projection

The isometric conversion belongs in `client`.

Example:

```rust
fn game_to_bevy(position: WorldPosition) -> Vec2 {
    Vec2::new(
        (position.x - position.y) * TILE_WIDTH * 0.5,
        -(position.x + position.y) * TILE_HEIGHT * 0.5,
    )
}
```

Mouse picking may also convert Bevy coordinates back into logical/grid coordinates.

That conversion is input/rendering logic, not authoritative server logic.

---

# 32. Mapping game units to Bevy entities

The server/game uses:

```rust
UnitId
```

The Bevy client can have:

```rust
#[derive(Component)]
struct GameUnitId(UnitId);
```

Concept:

```text
Server:
UnitId(42)
position = WorldPosition(...)

Client:
Bevy Entity
├── GameUnitId(UnitId(42))
├── Sprite
├── Transform
├── Animation
└── Selection visuals
```

Never use Bevy `Entity` as the network/persistent ID.

---

# 33. First Bevy milestone

Once headless movement and LocalConnection work:

1. Draw the same logical map in isometric view.
2. Render one server-owned unit.
3. Right-click a walkable tile.
4. Convert clicked tile to `GridPosition`.
5. Send:

```rust
ClientCommand::MoveUnits
```

6. Local server calculates path.
7. GameWorld moves the unit.
8. Client receives logical position updates.
9. Bevy updates the unit `Transform`.

The visual behavior should resemble the old prototype, but the architecture should now be:

```text
OLD PROTOTYPE

mouse
 ↓
Bevy pathfinding
 ↓
Bevy Transform movement
```

versus:

```text
NEW PROJECT

mouse
 ↓
ClientCommand
 ↓
LocalConnection
 ↓
GameWorld
 ↓
pathfinding
 ↓
logical movement
 ↓
server update
 ↓
Bevy isometric projection
 ↓
Transform
```

---

# 34. Multiple units and selection

Only after the first server-owned unit works.

Next:

1. Spawn three units.
2. Left-click a unit to select it.
3. Use Bevy marker component:

```rust
#[derive(Component)]
struct Selected;
```

4. Show a selection indicator.
5. Right-click sends a move command for the selected logical `UnitId`.
6. Only selected unit moves.

Do not add drag selection yet.

Do not add formations yet.

Do not solve unit collision yet.

---

# 35. Combat milestone

After movement/selection:

Add authoritative:

- Health.
- Attack range.
- Attack cooldown.
- Attack target.
- Death.

Flow:

```text
select unit
 ↓
command attack
 ↓
server validates ownership and target
 ↓
unit moves into range
 ↓
server attacks on ticks
 ↓
health decreases
 ↓
death
```

---

# 36. Resource gathering milestone

Turn tree from visual scenery into authoritative game data.

Flow:

```text
select worker
 ↓
Gather command
 ↓
server pathfinds near resource
 ↓
worker gathers
 ↓
resource amount decreases
 ↓
player receives wood
```

---

# 37. Base / island system

Use the same game/map engine.

Potential logical map type:

```rust
pub enum MapKind {
    OfflineBase {
        owner: PlayerId,
    },
    Island,
}
```

Offline base:

- Private.
- Safe.
- No PvP.
- Persistent.

Island:

- Shared.
- PvP/PvE.
- Persistent.

Do not build two completely separate engines.

---

# 38. Extraction milestone

Later implement server-authoritative extraction:

```text
StartExtraction command
 ↓
server validates
 ↓
ReturnBeacon created
 ↓
countdown starts
 ↓
other players can contest
 ↓
success
 ↓
units/resources transferred to base
```

---

# 39. Things not to implement yet

Do not introduce these during the first clean-repo milestones:

- Real networking.
- Tokio in core simulation.
- Database.
- Login/authentication.
- Persistence.
- Procedural generation.
- Chunk streaming.
- Advanced pathfinding.
- Unit collision avoidance.
- Formations.
- Complex server ECS.
- Complex replication.
- Production asset pipeline.
- Real art.
- Clans.
- Large-scale PvP.
- Matchmaking.
- Anti-cheat beyond server authority.

---

# 40. First clean-repository milestone

The first real milestone should result in:

```text
✓ Rust workspace
✓ game crate with no Bevy dependency
✓ protocol crate with no Bevy dependency
✓ client crate using Bevy
✓ server crate exists but may initially be minimal
✓ custom GridPosition
✓ custom WorldPosition
✓ 8×8 logical GameMap
✓ Grass / Water terrain
✓ walkability
✓ BFS pathfinding
✓ tests for map/pathfinding
✓ UnitId
✓ one authoritative Unit
✓ GameWorld
✓ fixed server tick
✓ headless movement
✓ ClientCommand::MoveUnits
✓ ServerMessage or equivalent state update
✓ ServerConnection abstraction
✓ LocalConnection
✓ Bevy renders the map
✓ Bevy renders the server-owned unit
✓ right-click sends movement command
✓ authoritative GameWorld performs movement
✓ Bevy only visualizes the result
```

No tree is required yet.

No multiple selection is required yet.

No real network is required yet.

---

# 41. Recommended implementation order

Codex should guide me through these steps one at a time.

## Step 1 — Create workspace

Create:

```text
game
protocol
client
server
```

Ensure:

```bash
cargo check --workspace
```

passes.

Explain Cargo workspace dependencies.

---

## Step 2 — Implement game coordinate types

Add:

```text
GridPosition
WorldPosition
```

Add small tests if useful.

Explain why these should not use Bevy types.

---

## Step 3 — Implement GameMap

Add:

```text
width
height
terrain Vec
is_inside
terrain lookup
is_walkable
```

Use only:

```text
Grass
Water
```

Write tests.

---

## Step 4 — Implement pathfinding

Port/adapt BFS.

Target:

```rust
find_path(&GameMap, start, goal)
```

Tests:

- valid path.
- path avoids water.
- blocked target fails.
- start == goal.
- outside-map destination fails.

---

## Step 5 — Implement Unit + GameWorld

Add:

```text
UnitId
Unit
GameWorld
```

Spawn one unit.

No Bevy involved.

---

## Step 6 — Implement authoritative movement

Add:

```text
Movement
GameWorld::tick
```

Move `WorldPosition` along a grid path.

Write a headless test proving a unit reaches its target.

---

## Step 7 — Add protocol command

Add:

```rust
ClientCommand::MoveUnits
```

Move unit movement initiation behind:

```rust
GameWorld::handle_command(...)
```

instead of calling movement directly.

---

## Step 8 — Add server update representation

Add the simplest useful form of:

```text
ServerMessage
```

or snapshot.

Do not overengineer replication.

---

## Step 9 — Add LocalConnection

Add queue-based communication.

Do not use Tokio.

Do not use threads.

Client/server semantics should still be non-immediate.

---

## Step 10 — Add Bevy rendering

Now add Bevy client.

Render:

- 8×8 isometric map.
- one logical unit.

Keep isometric projection in client code only.

---

## Step 11 — Add right-click movement

Mouse input should:

```text
right click
 ↓
grid position
 ↓
ClientCommand::MoveUnits
 ↓
LocalConnection
```

It must not directly call pathfinding.

---

## Step 12 — Sync rendering

Client reads authoritative updates.

Update Bevy `Transform` from `WorldPosition`.

At this point the first milestone is complete.

---

# 42. Coding style for Codex

When helping me:

- Make small changes.
- Prefer one architectural concept per step.
- Explain the reasoning first.
- Explain Rust syntax/concepts that may be unfamiliar.
- Do not dump a huge finished architecture at once.
- Keep code idiomatic but beginner-readable.
- Avoid clever abstractions.
- Avoid unnecessary traits/generics.
- Preserve type safety.
- Prefer explicit code over highly generic frameworks.
- Add tests for Bevy-independent logic.
- Do not add dependencies unless they solve a current problem.
- Do not introduce async before network work begins.
- Do not introduce networking before LocalConnection is proven.
- Do not introduce serialization before network work requires it.
- Do not introduce persistence before gameplay requires it.

When there is a choice between:

```text
more flexible architecture
```

and:

```text
simpler architecture that still preserves the client/server boundary
```

choose the simpler one.

---

# 43. First instruction for Codex

Start with a **new blank repository**.

Do not modify `beavy-learn-1`.

Treat `beavy-learn-1` only as a reference for previously learned isometric rendering/pathfinding ideas.

First guide me through:

> Creating the Rust workspace and implementing the Bevy-independent `GridPosition`, `WorldPosition`, `GameMap`, and pathfinding layer.

Requirements:

1. Start from an empty repository.
2. Create the workspace with `game`, `protocol`, `client`, and `server` crates.
3. Do not add real networking.
4. Do not add Tokio.
5. Do not add a database.
6. Do not add Bevy types to `game` or `protocol`.
7. Implement a simple 8×8 `GameMap`.
8. Add Grass and Water.
9. Implement walkability.
10. Implement BFS pathfinding.
11. Add tests.
12. Keep the client/server crates minimal for now.
13. Explain every important Rust or architectural decision.
14. Stop after this first milestone and let me review/understand it before continuing to `GameWorld`.

The next milestone after that will be:

> `UnitId` + `Unit` + `GameWorld` + fixed tick + headless movement.

After that:

> `ClientCommand` + `ServerMessage` + `LocalConnection`.

Only after those work should Bevy rendering and input be connected to the authoritative local server.
