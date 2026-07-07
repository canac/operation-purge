# Parity ledger (quirks registry)

> **Generated file — do not edit by hand.** Regenerate with
> `./scripts/gen_quirks.sh` after adding or moving any
> `// PARITY:` / `// GUARD:` / `// FIX:` comment.

Every intentional divergence from — or bug-for-bug reproduction of —
the 2011 C++/Direct3D9 original is pinned in the source with one of
three tags, each citing the C++ ground truth by `file:line`
(`~/Downloads/programs/OperationPurge`). This table is that ledger,
harvested straight from those comments.

| Tag | Meaning |
|---|---|
| `PARITY` | Behaviour deliberately reproduced bug-for-bug from the C++. |
| `GUARD`  | A hardening/validation check the original lacked. |
| `FIX`    | A corrected divergence — a C++ bug **not** reproduced. |

**179** ledger entries found across `src tests`.

## Parity quirks (`PARITY`) — 176

| Rust `file:line` | C++ citation | Note |
|---|---|---|
| `src/app/mod.rs:276` | `—` | empty New Game password enables cheats (design §1.3). |
| `src/app/mod.rs:658` | `cMainWindow.cpp:923` | ExecuteCommands runs every frame (`cMainWindow.cpp:923`, |
| `src/audio/mod.rs:31` | `—` | `IDirectSoundBuffer::SetVolume` takes hundredths of a decibel |
| `src/audio/mod.rs:341` | `—` | `cWaveFile` loaded whole 8-bit-unsigned / 16-bit-signed mono PCM |
| `src/audio/mod.rs:531` | `—` | EnemyVisible checked/played BEFORE Targeted (code over docs). |
| `src/audio/spatial.rs:27` | `—` | reproduces the DS3D inverse-distance model with `flMinDistance |
| `src/audio/spatial.rs:47` | `—` | DirectSound uses a **left-handed** coordinate system, so the |
| `src/audio/spatial.rs:74` | `—` | `ListenerData` in `cOperationPurge::ExecuteCommands` |
| `src/audio/spatial.rs:172` | `—` | DS3D is left-handed. Facing +Y, right = Z × Y = -X, so a |
| `src/data/builtin.rs:9` | `cOperationPurge.cpp:2153-2199` | verified 11-value record, `cOperationPurge.cpp:2153-2199` (+ |
| `src/data/mod.rs:184` | `—` | unused for turning (rate-limit code commented out, |
| `src/data/mod.rs:208` | `—` | only consumed by dead code (`cObstaclePathFinder`, not |
| `src/data/mod.rs:246` | `—` | an ammo center with no entry for a fired projectile type has |
| `src/data/mod.rs:343` | `—` | a *delay* between emits (seconds, or distance units when |
| `src/data/values.rs:18` | `—` | alpha is forced opaque here for every `[colors]` field except |
| `src/game/mod.rs:36` | `—` | game speed starts paused at `0.0` after robot selection |
| `src/game/mod.rs:42` | `—` | the enemy-visible HUD/alarm poll runs every 0.25 s of *game* time |
| `src/game/mod.rs:46` | `—` | `cOperationPurge::eObserverType` — which vantage the 3D camera + |
| `src/game/mod.rs:168` | `—` | the sim is frozen in the 2D map and behind any modal |
| `src/game/mod.rs:194` | `—` | `m_bForceChange` — set by `Enter2DMode(true)` when the active |
| `src/game/mod.rs:214` | `cOperationPurge.cpp:2037-2061` | `cOperationPurge::Enter2DMode` (`cOperationPurge.cpp:2037-2061`). |
| `src/game/mod.rs:229` | `cOperationPurge.cpp:2015-2034` | `cOperationPurge::GetRobotUnderPoint` (`cOperationPurge.cpp:2015-2034`) |
| `src/game/mod.rs:241` | `cOperationPurge.cpp:957-966` | `commandSelectObserverRobot` handler (`cOperationPurge.cpp:957-966`) |
| `src/game/mod.rs:260` | `cOperationPurge.cpp:968-977` | `commandSelectObserverPosition` handler (`cOperationPurge.cpp:968-977`). |
| `src/game/mod.rs:269` | `cOperationPurge.cpp:979-1018` | `commandSetObserver` handler (`cOperationPurge.cpp:979-1018`). |
| `src/game/mod.rs:287` | `cOperationPurge.cpp:1020-1035` | `commandExit2DMode` handler (`cOperationPurge.cpp:1020-1035`). |
| `src/game/mod.rs:348` | `—` | `cOperationPurge::StartGame` restart path |
| `src/game/mod.rs:381` | `cOperationPurge.cpp:880-1225` | `cOperationPurge::ExecuteCommands` (`cOperationPurge.cpp:880-1225`). |
| `src/game/mod.rs:404` | `—` | EndGame (Esc) / Restart (F4) are tested at the very top of |
| `src/game/mod.rs:459` | `—` | no upper bound; only clamped below at 0 (`:1145`). |
| `src/game/mod.rs:524` | `—` | game-over check at the END of `ExecuteCommands` |
| `src/game/mod.rs:543` | `cOperationPurge.cpp:1064-1120` | the free-observer command block (`cOperationPurge.cpp:1064-1120` |
| `src/game/mod.rs:648` | `cOperationPurge.cpp:1227-1420` | `cOperationPurge::FrameChange` (`cOperationPurge.cpp:1227-1420`). |
| `src/game/mod.rs:655` | `—` | no objects move in the 2D map (`:1229-1233`); the sim is |
| `src/game/mod.rs:694` | `—` | the targeted / above-center / enemy-visible flags computed in |
| `src/game/mod.rs:733` | `cOperationPurge.cpp:1596-1634` | `GetVisibleRobots` (`cOperationPurge.cpp:1596-1634`) restricted |
| `src/game/mod.rs:860` | `cOperationPurge.cpp:3034-3056` | `cOperationPurge::SetupScene` view pose (`cOperationPurge.cpp:3034-3056`) |
| `src/game/newgame.rs:23` | `—` | an empty password string enables cheats |
| `src/game/newgame.rs:30` | `cMainWindow.cpp:758` | `szPassword[40]` (`cMainWindow.cpp:758`) — the Win32 edit control |
| `src/game/newgame.rs:77` | `—` | Cancel behaviour differs by whether a game is already running |
| `src/game/newgame.rs:103` | `cMainWindow.cpp:760-761` | empty password ⇒ cheats on (`cMainWindow.cpp:760-761`). |
| `src/game/screens.rs:28` | `—` | `cOperationPurge::ExecuteCommands` game-over message text |
| `src/input/commands.rs:12` | `—` | variant order matches `cGameData::s_aszCommandsNames` |
| `src/input/commands.rs:217` | `cGameData.cpp:17-54` | pins the eCommand ABI (cGameData.cpp:17-54). Index N here |
| `src/input/commands.rs:251` | `—` | bExactModifiers = false — a command with no required |
| `src/input/keys.rs:491` | `—` | extra buttons don't bind (only 3 tracked in the original). |
| `src/math/intersect.rs:42` | `Intersect.h:107` | `Intersect.h:107,115` — literally `1.0`, the same value as a |
| `src/math/intersect.rs:1071` | `—` | a line that passes fully through a symmetric box (both |
| `src/math/mod.rs:14` | `Functions.cpp:8` | `Functions.cpp:8` `Equals` uses an **absolute** epsilon of |
| `src/math/mod.rs:23` | `Functions.cpp:83-86` | `Functions.cpp:83-86` — `fabsf (fNumber1 - fNumber2) <= COMPARE_TOLERANCE` |
| `src/math/mod.rs:30` | `Functions.cpp:56-62` | `Functions.cpp:56-62` `Between` — inclusive range test in either |
| `src/math/mod.rs:43` | `Functions.cpp:78-81` | `Functions.cpp:78-81` `Sign` — `fNumber >= 0.0f ? 1.0f : -1.0f` |
| `src/math/mod.rs:55` | `Functions.cpp:64-75` | `Functions.cpp:64-75` `Floor` — a truncation-based `floor` that |
| `src/math/mod.rs:95` | `Functions.cpp:191-200` | `Functions.cpp:191-200` `GetAngle(pt2Point1, pt2Point2)` = |
| `src/math/mod.rs:109` | `Functions.cpp:157-164` | `Functions.cpp:157-164` `SphereCoord` (source-map §3.7) — |
| `src/math/mod.rs:127` | `Functions.cpp:166-174` | `Functions.cpp:166-174` `AcceleratePoint(fTime, pt3Position, |
| `src/math/mod.rs:157` | `Functions.cpp:83-86` | Functions.cpp:83-86 uses `<=`, so a difference of exactly |
| `src/maze/mod.rs:65` | `cMaze.cpp:179-204` | `cMaze::OpenMaze` (`cMaze.cpp:179-204`) reads the 8-byte |
| `src/maze/search.rs:27` | `—` | the `strchr` anchor jump is a raw byte search — it is **not** |
| `src/path/astar.rs:214` | `cPathFinder.cpp:294` | `cPathFinder.cpp:294` — only skip if the node is |
| `src/path/astar.rs:250` | `cPathFinder.cpp:225-237` | `cPathFinder.cpp:225-237` — walk parents back to the |
| `src/path/astar.rs:341` | `—` | g/h are squared distance, not Euclidean distance — |
| `src/path/mod.rs:154` | `cBuilding.cpp:164-208` | `cBuilding::IntersectRect` (`cBuilding.cpp:164-208`) — |
| `src/path/mod.rs:185` | `—` | `cBuilding::IntersectMovingRect` → `GetIntersectedWalls` |
| `src/path/mod.rs:194` | `—` | `cBuilding::GetIntersectedWallsPortion` |
| `src/path/mod.rs:258` | `cPathFinder.cpp:373-401` | `cPathFinder::IsPointVisible` (`cPathFinder.cpp:373-401`). |
| `src/path/mod.rs:288` | `cPathFinder.cpp:454-480` | `cHighLevelPathFinder::GetNeighbors` (`cPathFinder.cpp:454-480`). |
| `src/path/mod.rs:313` | `cPathFinder.cpp:521-595` | `cLowLevelPathFinder::GetNeighbors` (`cPathFinder.cpp:521-595`). |
| `src/path/mod.rs:386` | `cPathFinder.cpp:428-451` | `cHighLevelPathFinder::FindPath` (`cPathFinder.cpp:428-451`). |
| `src/path/mod.rs:401` | `cPathFinder.cpp:501-518` | `cLowLevelPathFinder::FindPath` (`cPathFinder.cpp:501-518`). |
| `src/path/nodes.rs:28` | `cPathFinder.cpp:14` | `cPathFinder.cpp:14` `s_nNodesPerPoint = 256` — the hash |
| `src/path/nodes.rs:66` | `cNodeList.cpp:19-23` | `cNodeList::Reset` (`cNodeList.cpp:19-23`) — clears the |
| `src/profiler.rs:101` | `—` | `cProfileNode::GetSubNode` linear-scans siblings by name. |
| `src/render/d3dx.rs:512` | `—` | NaN in -> NaN out (no finiteness guard); the "NaN on zero |
| `src/render/mesh.rs:31` | `—` | normals are stored and uploaded exactly as computed — never |
| `src/render/robot3d.rs:64` | `cRobot3D.cpp:188` | z compressed by fBodyHeight/fRadius (cRobot3D.cpp:188). |
| `src/render/robot3d.rs:66` | `—` | normal = the (un-normalized) scaled position. |
| `src/render/robot3d.rs:103` | `—` | un-normalized. |
| `src/render/shaders.rs:65` | `—` | FFP Gouraud lighting, one white point light AT THE EYE. |
| `src/render/shaders.rs:68` | `—` | N_view = mat3(world_view) * N, NO inverse-transpose, N NOT normalized. |
| `src/rng.rs:36` | `—` | `fRand() = rand() / (RAND_MAX + 1)` — a value in `[0, 1)` |
| `src/sim/ai.rs:141` | `—` | two `fRand ()` draws in this exact order — first |
| `src/sim/ai.rs:223` | `—` | the whole-map ordering + per-robot `CalcNewPositionAI` decision |
| `src/sim/ai.rs:534` | `cAIRobot.cpp:87-95` | PreventCollision block (cAIRobot.cpp:87-95) is commented |
| `src/sim/ai.rs:684` | `cAIRobot.cpp:410-426` | v²/(2d) vs friction/brake decel (cAIRobot.cpp:410-426). |
| `src/sim/ai.rs:737` | `cRobot.cpp:336-370` | `cRobot::Fire (const cRobot&)` (`cRobot.cpp:336-370`): solve |
| `src/sim/ai.rs:798` | `cAIRobot.cpp:765` | lookahead min(2, len-1) rooms (cAIRobot.cpp:765). |
| `src/sim/ai.rs:885` | `cAIRobot.cpp:105-162` | `cAIRobot::MoveAI` turning block (`cAIRobot.cpp:105-162`). |
| `src/sim/ai.rs:910` | `—` | EnterIdleMode overwrites direction_desired with the |
| `src/sim/ai.rs:918` | `cAIRobot.cpp:545-620` | `cAIRobot::ChangeDesiredDirection` (`cAIRobot.cpp:545-620`). |
| `src/sim/ai.rs:979` | `cAIRobot.cpp:135-148` | the wall-bounce heading snap (`cAIRobot.cpp:135-148`). A `planeX` |
| `src/sim/center.rs:109` | `cCenter.cpp:78-91` | `cCenter::IsWithinBoundries` (`cCenter.cpp:78-91`) — the |
| `src/sim/center.rs:121` | `cCenter.cpp:99-104` | `cCenter::IsTransferPossible` (`cCenter.cpp:99-104`) — the |
| `src/sim/idgen.rs:18` | `cIDGenerator.cpp:5` | `cIDGenerator.cpp:5` — both ternary branches evaluate to this, |
| `src/sim/idgen.rs:28` | `—` | the original constructor's `nBits` argument is ignored |
| `src/sim/idgen.rs:41` | `cIDGenerator.cpp:18-31` | `cIDGenerator.cpp:18-31` `Next()` verbatim: |
| `src/sim/idgen.rs:74` | `cIDGenerator.cpp:34-38` | `cIDGenerator.cpp:34-38` `Reset()`. |
| `src/sim/idgen.rs:152` | `—` | any nBits argument must produce the identical sequence. |
| `src/sim/particles.rs:40` | `Functions.cpp:96-99` | `fRandVariance` (`Functions.cpp:96-99`). |
| `src/sim/particles.rs:45` | `Functions.cpp:102-105` | `fRandMinMax` (`Functions.cpp:102-105`). |
| `src/sim/particles.rs:71` | `cParticle.cpp:10-27` | `cParticle::cParticle` (`cParticle.cpp:10-27`). Constructor |
| `src/sim/particles.rs:102` | `cParticle.cpp:52-57` | `cParticle::Move` (`cParticle.cpp:52-57`). |
| `src/sim/particles.rs:109` | `cParticle.cpp:60-63` | `cParticle::IsLifeExpired` (`cParticle.cpp:60-63`). |
| `src/sim/particles.rs:114` | `cParticle.cpp:66-69` | `cParticle::GetLifetimePortion` (`cParticle.cpp:66-69`). |
| `src/sim/particles.rs:121` | `cParticle.cpp:88-121` | `cParticle::GetStage` (`cParticle.cpp:88-121`). Returns |
| `src/sim/particles.rs:142` | `cParticle.cpp:124-135` | `cParticle::GetSize` (`cParticle.cpp:124-135`). |
| `src/sim/particles.rs:151` | `cParticle.cpp:138-141` | `cParticle::GetRotation` (`cParticle.cpp:138-141`). |
| `src/sim/particles.rs:156` | `cParticle.cpp:144-155` | `cParticle::GetColor` (`cParticle.cpp:144-155`) — full RGBA |
| `src/sim/particles.rs:186` | `—` | `cOperationPurge::AddParticle` (reconstructed per design §7 / |
| `src/sim/particles.rs:270` | `—` | the shortest-arc rotation that maps +Z onto `emitter_velocity`, |
| `src/sim/particles.rs:313` | `cParticleEmitter.cpp:10-24` | `cParticleEmitter::cParticleEmitter` (`cParticleEmitter.cpp:10-24`). |
| `src/sim/particles.rs:341` | `cParticleEmitter.cpp:79-82` | `cParticleEmitter::IsLifeExpired` (`cParticleEmitter.cpp:79-82`). |
| `src/sim/particles.rs:346` | `cParticleEmitter.cpp:130-133` | `GetEmitDelay` (`cParticleEmitter.cpp:130-133`). |
| `src/sim/particles.rs:352` | `—` | `cParticleEmitter::GetEmitVelocity(vec3Emitter)` |
| `src/sim/particles.rs:373` | `—` | `cParticleEmitter::Move(fTimeElapsed)` |
| `src/sim/particles.rs:382` | `—` | `cParticleEmitter::Move(fTimeElapsed, pt3Position, vec3Velocity)` |
| `src/sim/particles.rs:408` | `—` | fEmitRate is a delay, consumed by sub-frame interpolation. |
| `src/sim/particles.rs:451` | `cParticleEffect.cpp:6-27` | `cParticleEffect::cParticleEffect` (`cParticleEffect.cpp:6-27`). |
| `src/sim/particles.rs:489` | `cParticleEffect.cpp:40-49` | `cParticleEffect::Move(fTimeElapsed)` (`cParticleEffect.cpp:40-49`). |
| `src/sim/particles.rs:497` | `—` | `cParticleEffect::Move(fTimeElapsed, pt3Position, vec3Velocity)` |
| `src/sim/particles.rs:515` | `cParticleEffect.cpp:65-68` | `cParticleEffect::IsFinished` (`cParticleEffect.cpp:65-68`). |
| `src/sim/particles.rs:520` | `cParticleEffect.cpp:71-97` | `cParticleEffect::RemoveEmitters` (`cParticleEffect.cpp:71-97`). |
| `src/sim/particles.rs:546` | `—` | D3DXVec3Normalize(0) -> NaN, no fallback fires. |
| `src/sim/particles.rs:553` | `—` | velocity along -Z -> w normalizes to 0 -> (0,0,1,0) |
| `src/sim/projectile.rs:103` | `cProjectile.cpp:108-135` | `cProjectile::Move` (`cProjectile.cpp:108-135`). Advances one |
| `src/sim/projectile.rs:123` | `cProjectile.cpp:139-157` | `cProjectile::IntersectRobot` (`cProjectile.cpp:139-157`). |
| `src/sim/projectile.rs:139` | `cProjectile.cpp:161-177` | `cProjectile::IntersectBuilding` (`cProjectile.cpp:161-177`). |
| `src/sim/projectile.rs:155` | `cProjectile.cpp:180-187` | `cProjectile::AddCollision` (`cProjectile.cpp:180-187`) — |
| `src/sim/projectile.rs:168` | `cProjectile.cpp:229-233` | `cProjectile::GetDamage` (`cProjectile.cpp:229-233`): |
| `src/sim/projectile.rs:176` | `—` | `cProjectile::DisplayHole` position/orientation |
| `src/sim/projectile.rs:224` | `—` | `cProjectile::GetTrajectory` (the static aim solver, |
| `src/sim/robot.rs:24` | `—` | `Make2D (cRobot3D::GetSize ())` = `(fBarrelLength*2, fBarrelLength*2)` |
| `src/sim/robot.rs:29` | `—` | robot 3D box height `fBodyHeight + fTreadRadius*2` |
| `src/sim/robot.rs:33` | `cRobot3D.cpp:40` | `cRobot3D::GetCenter ()` = `(0, 0, size.z * 0.5)` (`cRobot3D.cpp:40`). |
| `src/sim/robot.rs:60` | `cRobot.cpp:838-841` | `cRobot::tProjectileTypeState::FireReady` (`cRobot.cpp:838-841`) |
| `src/sim/robot.rs:309` | `cRobot.cpp:791-835` | `cRobot::GetMoveAccel` (`cRobot.cpp:791-835`). Returns |
| `src/sim/robot.rs:336` | `cRobot.cpp:94-136` | `cRobot::CalcNewPosition` (`cRobot.cpp:94-136`). Phase 1 — |
| `src/sim/robot.rs:356` | `cRobot.cpp:117-120` | instantaneous turn (rate-limit commented out, cRobot.cpp:117-120). |
| `src/sim/robot.rs:370` | `cRobot.cpp:226-275` | `cRobot::AddCollision` (`cRobot.cpp:226-275`). Records the |
| `src/sim/robot.rs:430` | `cRobot.cpp:145-223` | `cRobot::Move` (`cRobot.cpp:145-223`). Phase 3 — commits the |
| `src/sim/robot.rs:452` | `cRobot.cpp:488-513` | `cRobot::IntersectRobot` (`cRobot.cpp:488-513`). |
| `src/sim/robot.rs:488` | `cRobot.cpp:427-441` | `cRobot::GetProjectileFireType` (`cRobot.cpp:427-441`) — the |
| `src/sim/robot.rs:500` | `cRobot.cpp:444-458` | `cRobot::GetProjectileRestockType` (`cRobot.cpp:444-458`) — the |
| `src/sim/robot.rs:512` | `cRobot.cpp:279-313` | `cRobot::Fire (tProjectileTypeID)` (`cRobot.cpp:279-313`). |
| `src/sim/robot.rs:555` | `cRobot.cpp:461-471` | `cRobot::IncreaseAmmo` (`cRobot.cpp:461-471`) — clamped to the |
| `src/sim/robot.rs:586` | `—` | `cRobot::Fire (const cRobot&)` fire-alignment gate |
| `src/sim/robot.rs:594` | `cRobot3D.cpp:45-49` | `cRobot3D::GetFiringOffset` (`cRobot3D.cpp:45-49`): |
| `src/sim/world.rs:73` | `cOperationPurge.cpp:1190` | no move keys down => cruise (`cOperationPurge.cpp:1190`). |
| `src/sim/world.rs:188` | `cOperationPurge.cpp:153-306` | `cOperationPurge::StartGame` (`cOperationPurge.cpp:153-306`). |
| `src/sim/world.rs:219` | `—` | initial spawn point uses the robot's 3D footprint |
| `src/sim/world.rs:310` | `—` | dynamic effects never self-prune (they never reach |
| `src/sim/world.rs:320` | `—` | the original increments the 10-frame counter, then updates |
| `src/sim/world.rs:395` | `cOperationPurge.cpp:1424` | `cOperationPurge::AddProjectile` (`cOperationPurge.cpp:1424+`) — |
| `src/sim/world.rs:450` | `cOperationPurge.cpp:804-865` | `cOperationPurge::TestCollisions` (`cOperationPurge.cpp:804-865`) |
| `src/sim/world.rs:466` | `—` | security break against an infinite loop |
| `src/sim/world.rs:492` | `—` | `cOperationPurge::IntersectRobotObjects` |
| `src/sim/world.rs:520` | `—` | the "Move projectiles" pass of `MoveObjects` |
| `src/sim/world.rs:602` | `—` | `cOperationPurge::RemoveDestroyedRobots` |
| `src/sim/world.rs:632` | `cOperationPurge.cpp:868-877` | `cOperationPurge::Transfer` (`cOperationPurge.cpp:868-877`) |
| `src/sim/world.rs:640` | `—` | `cCenter::ChooseNewTransferRobot`/`GetTransferRobot` + |
| `src/sim/world.rs:748` | `—` | `cOperationPurge::GetNewTransferRobot` |
| `src/sim/world.rs:763` | `cOperationPurge.cpp:445-516` | `cOperationPurge::GetGameStatus` (`cOperationPurge.cpp:445-516`). |
| `src/sim/world.rs:808` | `cOperationPurge.cpp:2235-2309` | `cOperationPurge::SpawnRobots` (`cOperationPurge.cpp:2235-2309`). |
| `src/sim/world.rs:873` | `cOperationPurge.cpp:2347-2369` | `cOperationPurge::GetRectPosition` (`cOperationPurge.cpp:2347-2369`) |
| `src/sim/world.rs:896` | `—` | `cOperationPurge::IntersectRectObjects` |
| `src/sim/world.rs:908` | `—` | `cOperationPurge::InitializeCenterPositions` |
| `src/sim/world.rs:956` | `—` | `cOperationPurge::CreateCenterGroups` |
| `src/sim/world.rs:1023` | `—` | `cOperationPurge::GetCenterPosition` |
| `src/sim/world.rs:1059` | `Functions.cpp:96-99` | `fRandVariance` (`Functions.cpp:96-99`) — `median + rand01 * |
| `src/sim/world.rs:1077` | `—` | ported from `cBuilding.cpp`. Kept as free functions over |
| `src/sim/world.rs:1136` | `cBuilding.cpp:164-208` | `cBuilding::IntersectRect` (`cBuilding.cpp:164-208`) — static |
| `src/sim/world.rs:1171` | `—` | `cBuilding::IntersectMovingRect` -> `GetIntersectedWalls` |
| `src/sim/world.rs:1181` | `cBuilding.cpp:219-273` | `cBuilding::IntersectTrajectory` (`cBuilding.cpp:219-273`). |
| `src/sim/world.rs:1233` | `—` | `cBuilding::GetIntersectedWallsPortion` |
| `tests/game.rs:173` | `cOperationPurge.cpp:1190` | no move keys down ⇒ cruise (`cOperationPurge.cpp:1190`). |
| `tests/game.rs:398` | `—` | the 2D branch returns before the game-over check (`:1058`). |

## Hardening guards (`GUARD`) — 3

| Rust `file:line` | C++ citation | Note |
|---|---|---|
| `src/error.rs:15` | `—` | replaces `cGameData::ReadFileXML`'s ignored `TiXmlDocument::LoadFile` |
| `src/error.rs:46` | `—` | the C++ performs zero header validation — a signed `cx`/`cy` pair |
| `src/error.rs:85` | `—` | building meshes keep 16-bit indices (design §1.3); a mesh |

## Corrected divergences (`FIX`) — 0

| Rust `file:line` | C++ citation | Note |
|---|---|---|

