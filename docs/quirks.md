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

**186** ledger entries found across `src tests`.

## Parity quirks (`PARITY`) — 181

| Rust `file:line` | C++ citation | Note |
|---|---|---|
| `src/app/mod.rs:300` | `—` | empty New Game password enables cheats (design §1.3). |
| `src/app/mod.rs:604` | `cOperationPurge.cpp:2848-3030` | `Render3D` order (`cOperationPurge.cpp:2848-3030`): building; |
| `src/app/mod.rs:748` | `—` | the active robot is not drawn — the eye sits inside it |
| `src/app/mod.rs:943` | `cMainWindow.cpp:923` | ExecuteCommands runs every frame (`cMainWindow.cpp:923`, |
| `src/app/mod.rs:1006` | `—` | the cursor shows only on the 2D map / menus (`ShowCursor`, |
| `src/audio/mod.rs:30` | `—` | `IDirectSoundBuffer::SetVolume` takes hundredths of a decibel |
| `src/audio/mod.rs:479` | `—` | EnemyVisible checked/played BEFORE Targeted (code over docs). |
| `src/audio/spatial.rs:27` | `—` | reproduces the DS3D inverse-distance model with `flMinDistance |
| `src/audio/spatial.rs:47` | `—` | DirectSound uses a **left-handed** coordinate system, so the |
| `src/audio/spatial.rs:74` | `—` | `ListenerData` in `cOperationPurge::ExecuteCommands` |
| `src/audio/spatial.rs:172` | `—` | DS3D is left-handed. Facing +Y, right = Z × Y = -X, so a |
| `src/data/builtin.rs:15` | `cOperationPurge.cpp:2153-2199` | verified 11-value record, `cOperationPurge.cpp:2153-2199` (+ |
| `src/data/mod.rs:167` | `—` | unused for turning (rate-limit code commented out, |
| `src/data/mod.rs:190` | `—` | only consumed by dead code (`cObstaclePathFinder`, not |
| `src/data/mod.rs:221` | `—` | an ammo center with no entry for a fired projectile type has |
| `src/data/mod.rs:317` | `—` | a *delay* between emits (seconds, or distance units when |
| `src/data/values.rs:17` | `—` | alpha is forced opaque here for every `[colors]` field except |
| `src/game/mod.rs:37` | `—` | game speed starts paused at `0.0` after robot selection |
| `src/game/mod.rs:43` | `—` | the enemy-visible HUD/alarm poll runs every 0.25 s of *game* time |
| `src/game/mod.rs:47` | `—` | `MIN_VISIBLE_SIZE` — the smallest 2D-map window, in rooms |
| `src/game/mod.rs:51` | `—` | `cOperationPurge::eObserverType` — which vantage the 3D camera + |
| `src/game/mod.rs:165` | `cOperationPurge.cpp:144-147` | `SetGameParams` shows the whole building (`cOperationPurge.cpp:144-147`). |
| `src/game/mod.rs:183` | `—` | the view-window update at the top of `Render2D` |
| `src/game/mod.rs:218` | `cOperationPurge.cpp:3148-3163` | `cOperationPurge::WinToLog` (`cOperationPurge.cpp:3148-3163`) — |
| `src/game/mod.rs:266` | `—` | the sim is frozen in the 2D map and behind any modal |
| `src/game/mod.rs:292` | `—` | `m_bForceChange` — set by `Enter2DMode(true)` when the active |
| `src/game/mod.rs:312` | `cOperationPurge.cpp:2037-2061` | `cOperationPurge::Enter2DMode` (`cOperationPurge.cpp:2037-2061`). |
| `src/game/mod.rs:327` | `cOperationPurge.cpp:2015-2034` | `cOperationPurge::GetRobotUnderPoint` (`cOperationPurge.cpp:2015-2034`) |
| `src/game/mod.rs:339` | `cOperationPurge.cpp:957-966` | `commandSelectObserverRobot` handler (`cOperationPurge.cpp:957-966`) |
| `src/game/mod.rs:358` | `cOperationPurge.cpp:968-977` | `commandSelectObserverPosition` handler (`cOperationPurge.cpp:968-977`). |
| `src/game/mod.rs:367` | `cOperationPurge.cpp:979-1018` | `commandSetObserver` handler (`cOperationPurge.cpp:979-1018`). |
| `src/game/mod.rs:385` | `cOperationPurge.cpp:1020-1035` | `commandExit2DMode` handler (`cOperationPurge.cpp:1020-1035`). |
| `src/game/mod.rs:450` | `—` | `cOperationPurge::StartGame` restart path |
| `src/game/mod.rs:484` | `cOperationPurge.cpp:880-1225` | `cOperationPurge::ExecuteCommands` (`cOperationPurge.cpp:880-1225`). |
| `src/game/mod.rs:507` | `—` | EndGame (Esc) / Restart (F4) are tested at the very top of |
| `src/game/mod.rs:592` | `—` | no upper bound; only clamped below at 0 (`:1145`). |
| `src/game/mod.rs:658` | `cOperationPurge.cpp:1064-1120` | the free-observer command block (`cOperationPurge.cpp:1064-1120` |
| `src/game/mod.rs:763` | `cOperationPurge.cpp:1227-1420` | `cOperationPurge::FrameChange` (`cOperationPurge.cpp:1227-1420`). |
| `src/game/mod.rs:770` | `—` | no objects move in the 2D map (`:1229-1233`); the sim is |
| `src/game/mod.rs:813` | `—` | the win/lose/draw check at the END of `FrameChange` |
| `src/game/mod.rs:830` | `—` | the targeted / above-center / enemy-visible flags computed in |
| `src/game/mod.rs:869` | `cOperationPurge.cpp:1596-1634` | `GetVisibleRobots` (`cOperationPurge.cpp:1596-1634`) restricted |
| `src/game/mod.rs:1001` | `cOperationPurge.cpp:3034-3056` | `cOperationPurge::SetupScene` view pose (`cOperationPurge.cpp:3034-3056`) |
| `src/game/newgame.rs:23` | `—` | an empty password string enables cheats |
| `src/game/newgame.rs:30` | `cMainWindow.cpp:758` | `szPassword[40]` (`cMainWindow.cpp:758`) — the Win32 edit control |
| `src/game/newgame.rs:77` | `—` | Cancel behaviour differs by whether a game is already running |
| `src/game/newgame.rs:103` | `cMainWindow.cpp:760-761` | empty password ⇒ cheats on (`cMainWindow.cpp:760-761`). |
| `src/game/screens.rs:26` | `—` | `cOperationPurge::ExecuteCommands` game-over message text |
| `src/input/commands.rs:13` | `—` | variant order matches `cGameData::s_aszCommandsNames` |
| `src/input/commands.rs:178` | `—` | transcribed from the original `GameData.xml` `fire_command`s. |
| `src/input/commands.rs:255` | `cGameData.cpp:17-54` | pins the eCommand ABI (cGameData.cpp:17-54). Index N here |
| `src/input/commands.rs:289` | `—` | bExactModifiers = false — a command with no required |
| `src/input/keys.rs:470` | `—` | extra buttons don't bind (only 3 tracked in the original). |
| `src/math/intersect.rs:42` | `Intersect.h:107` | `Intersect.h:107,115` — literally `1.0`, the same value as a |
| `src/math/intersect.rs:984` | `—` | a line that passes fully through a symmetric box (both |
| `src/math/mod.rs:14` | `Functions.cpp:8` | `Functions.cpp:8` `Equals` uses an **absolute** epsilon of |
| `src/math/mod.rs:23` | `Functions.cpp:83-86` | `Functions.cpp:83-86` — `fabsf (fNumber1 - fNumber2) <= COMPARE_TOLERANCE` |
| `src/math/mod.rs:30` | `Functions.cpp:56-62` | `Functions.cpp:56-62` `Between` — inclusive range test in either |
| `src/math/mod.rs:43` | `Functions.cpp:78-81` | `Functions.cpp:78-81` `Sign` — `fNumber >= 0.0f ? 1.0f : -1.0f` |
| `src/math/mod.rs:55` | `Functions.cpp:64-75` | `Functions.cpp:64-75` `Floor` — a truncation-based `floor` that |
| `src/math/mod.rs:95` | `Functions.cpp:191-200` | `Functions.cpp:191-200` `GetAngle(pt2Point1, pt2Point2)` = |
| `src/math/mod.rs:109` | `Functions.cpp:157-164` | `Functions.cpp:157-164` `SphereCoord` (source-map §3.7) — |
| `src/math/mod.rs:127` | `Functions.cpp:166-174` | `Functions.cpp:166-174` `AcceleratePoint(fTime, pt3Position, |
| `src/math/mod.rs:157` | `Functions.cpp:83-86` | Functions.cpp:83-86 uses `<=`, so a difference of exactly |
| `src/maze/building.rs:512` | `—` | ported from `cBuilding.cpp`. Wall existence is read through |
| `src/maze/building.rs:564` | `cBuilding.cpp:164-208` | `cBuilding::IntersectRect` (`cBuilding.cpp:164-208`) — static |
| `src/maze/building.rs:599` | `—` | `cBuilding::IntersectMovingRect` -> `GetIntersectedWalls` |
| `src/maze/building.rs:609` | `cBuilding.cpp:219-273` | `cBuilding::IntersectTrajectory` (`cBuilding.cpp:219-273`). |
| `src/maze/building.rs:658` | `—` | `cBuilding::GetIntersectedWallsPortion` |
| `src/maze/mod.rs:65` | `cMaze.cpp:179-204` | `cMaze::OpenMaze` (`cMaze.cpp:179-204`) reads the 8-byte |
| `src/maze/search.rs:27` | `—` | the `strchr` anchor jump is a raw byte search — it is **not** |
| `src/path/astar.rs:204` | `cPathFinder.cpp:294` | `cPathFinder.cpp:294` — only skip if the node is |
| `src/path/astar.rs:240` | `cPathFinder.cpp:225-237` | `cPathFinder.cpp:225-237` — walk parents back to the |
| `src/path/mod.rs:110` | `cPathFinder.cpp:373-401` | `cPathFinder::IsPointVisible` (`cPathFinder.cpp:373-401`). |
| `src/path/mod.rs:134` | `cPathFinder.cpp:454-480` | `cHighLevelPathFinder::GetNeighbors` (`cPathFinder.cpp:454-480`). |
| `src/path/mod.rs:159` | `cPathFinder.cpp:521-595` | `cLowLevelPathFinder::GetNeighbors` (`cPathFinder.cpp:521-595`). |
| `src/path/mod.rs:253` | `cPathFinder.cpp:501-518` | `cLowLevelPathFinder::FindPath` (`cPathFinder.cpp:501-518`). |
| `src/path/nodes.rs:28` | `cPathFinder.cpp:14` | `cPathFinder.cpp:14` `s_nNodesPerPoint = 256` — the hash |
| `src/path/nodes.rs:66` | `cNodeList.cpp:19-23` | `cNodeList::Reset` (`cNodeList.cpp:19-23`) — clears the |
| `src/profiler.rs:101` | `—` | `cProfileNode::GetSubNode` linear-scans siblings by name. |
| `src/render/d3dx.rs:512` | `—` | NaN in -> NaN out (no finiteness guard); the "NaN on zero |
| `src/render/mesh.rs:31` | `—` | normals are stored and uploaded exactly as computed — never |
| `src/render/overlay.rs:303` | `—` | `cOverlay2D::StartRender` visibility bounds — whole rooms fully |
| `src/render/overlay.rs:417` | `cOperationPurge.cpp:2762-2835` | the building half of `Render2D` (`cOperationPurge.cpp:2762-2835`): |
| `src/render/overlay.rs:469` | `—` | `cOverlay2D::RenderCrosshair` — screen-centered, 3D mode only. |
| `src/render/robot3d.rs:64` | `cRobot3D.cpp:188` | z compressed by fBodyHeight/fRadius (cRobot3D.cpp:188). |
| `src/render/robot3d.rs:66` | `—` | normal = the (un-normalized) scaled position. |
| `src/render/robot3d.rs:103` | `—` | un-normalized. |
| `src/render/shaders.rs:67` | `—` | FFP Gouraud lighting, one white point light AT THE EYE. |
| `src/render/shaders.rs:70` | `—` | N_view = mat3(world_view) * N, NO inverse-transpose, N NOT normalized. |
| `src/rng.rs:36` | `—` | `fRand() = rand() / (RAND_MAX + 1)` — a value in `[0, 1)` |
| `src/sim/ai.rs:141` | `—` | two `fRand ()` draws in this exact order — first |
| `src/sim/ai.rs:223` | `—` | the whole-map ordering + per-robot `CalcNewPositionAI` decision |
| `src/sim/ai.rs:535` | `cAIRobot.cpp:87-95` | PreventCollision block (cAIRobot.cpp:87-95) is commented |
| `src/sim/ai.rs:685` | `cAIRobot.cpp:410-426` | v²/(2d) vs friction/brake decel (cAIRobot.cpp:410-426). |
| `src/sim/ai.rs:738` | `cRobot.cpp:336-370` | `cRobot::Fire (const cRobot&)` (`cRobot.cpp:336-370`): solve |
| `src/sim/ai.rs:799` | `cAIRobot.cpp:765` | lookahead min(2, len-1) rooms (cAIRobot.cpp:765). |
| `src/sim/ai.rs:886` | `cAIRobot.cpp:105-162` | `cAIRobot::MoveAI` turning block (`cAIRobot.cpp:105-162`). |
| `src/sim/ai.rs:911` | `—` | EnterIdleMode overwrites direction_desired with the |
| `src/sim/ai.rs:919` | `cAIRobot.cpp:545-620` | `cAIRobot::ChangeDesiredDirection` (`cAIRobot.cpp:545-620`). |
| `src/sim/ai.rs:980` | `cAIRobot.cpp:135-148` | the wall-bounce heading snap (`cAIRobot.cpp:135-148`). A `planeX` |
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
| `src/sim/particles.rs:275` | `—` | the shortest-arc rotation that maps +Z onto `emitter_velocity`, |
| `src/sim/particles.rs:318` | `cParticleEmitter.cpp:10-24` | `cParticleEmitter::cParticleEmitter` (`cParticleEmitter.cpp:10-24`). |
| `src/sim/particles.rs:346` | `cParticleEmitter.cpp:79-82` | `cParticleEmitter::IsLifeExpired` (`cParticleEmitter.cpp:79-82`). |
| `src/sim/particles.rs:351` | `cParticleEmitter.cpp:130-133` | `GetEmitDelay` (`cParticleEmitter.cpp:130-133`). |
| `src/sim/particles.rs:357` | `—` | `cParticleEmitter::GetEmitVelocity(vec3Emitter)` |
| `src/sim/particles.rs:378` | `—` | `cParticleEmitter::Move(fTimeElapsed)` |
| `src/sim/particles.rs:387` | `—` | `cParticleEmitter::Move(fTimeElapsed, pt3Position, vec3Velocity)` |
| `src/sim/particles.rs:413` | `—` | fEmitRate is a delay, consumed by sub-frame interpolation. |
| `src/sim/particles.rs:456` | `cParticleEffect.cpp:6-27` | `cParticleEffect::cParticleEffect` (`cParticleEffect.cpp:6-27`). |
| `src/sim/particles.rs:494` | `cParticleEffect.cpp:40-49` | `cParticleEffect::Move(fTimeElapsed)` (`cParticleEffect.cpp:40-49`). |
| `src/sim/particles.rs:502` | `—` | `cParticleEffect::Move(fTimeElapsed, pt3Position, vec3Velocity)` |
| `src/sim/particles.rs:520` | `cParticleEffect.cpp:65-68` | `cParticleEffect::IsFinished` (`cParticleEffect.cpp:65-68`). |
| `src/sim/particles.rs:525` | `cParticleEffect.cpp:71-97` | `cParticleEffect::RemoveEmitters` (`cParticleEffect.cpp:71-97`). |
| `src/sim/particles.rs:551` | `—` | D3DXVec3Normalize(0) -> NaN, no fallback fires. |
| `src/sim/particles.rs:558` | `—` | velocity along -Z -> w normalizes to 0 -> (0,0,1,0) |
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
| `src/sim/world.rs:69` | `cOperationPurge.cpp:1190` | no move keys down => cruise (`cOperationPurge.cpp:1190`). |
| `src/sim/world.rs:194` | `cOperationPurge.cpp:153-306` | `cOperationPurge::StartGame` (`cOperationPurge.cpp:153-306`). |
| `src/sim/world.rs:226` | `—` | initial spawn point uses the robot's 3D footprint |
| `src/sim/world.rs:326` | `—` | dynamic effects never self-prune (they never reach |
| `src/sim/world.rs:340` | `—` | the original increments the 10-frame counter, then updates |
| `src/sim/world.rs:415` | `cOperationPurge.cpp:1424` | `cOperationPurge::AddProjectile` (`cOperationPurge.cpp:1424+`) — |
| `src/sim/world.rs:474` | `cOperationPurge.cpp:804-865` | `cOperationPurge::TestCollisions` (`cOperationPurge.cpp:804-865`) |
| `src/sim/world.rs:490` | `—` | security break against an infinite loop |
| `src/sim/world.rs:516` | `—` | `cOperationPurge::IntersectRobotObjects` |
| `src/sim/world.rs:544` | `—` | the "Move projectiles" pass of `MoveObjects` |
| `src/sim/world.rs:641` | `—` | `cOperationPurge::RemoveDestroyedRobots` |
| `src/sim/world.rs:675` | `cOperationPurge.cpp:868-877` | `cOperationPurge::Transfer` (`cOperationPurge.cpp:868-877`) |
| `src/sim/world.rs:683` | `—` | `cCenter::ChooseNewTransferRobot`/`GetTransferRobot` + |
| `src/sim/world.rs:791` | `—` | `cOperationPurge::GetNewTransferRobot` |
| `src/sim/world.rs:806` | `cOperationPurge.cpp:445-516` | `cOperationPurge::GetGameStatus` (`cOperationPurge.cpp:445-516`). |
| `src/sim/world.rs:851` | `cOperationPurge.cpp:2235-2309` | `cOperationPurge::SpawnRobots` (`cOperationPurge.cpp:2235-2309`). |
| `src/sim/world.rs:921` | `cOperationPurge.cpp:2347-2369` | `cOperationPurge::GetRectPosition` (`cOperationPurge.cpp:2347-2369`) |
| `src/sim/world.rs:944` | `—` | `cOperationPurge::IntersectRectObjects` |
| `src/sim/world.rs:956` | `—` | `cOperationPurge::InitializeCenterPositions` |
| `src/sim/world.rs:1004` | `—` | `cOperationPurge::CreateCenterGroups` |
| `src/sim/world.rs:1071` | `—` | `cOperationPurge::GetCenterPosition` |
| `src/sim/world.rs:1107` | `Functions.cpp:96-99` | `fRandVariance` (`Functions.cpp:96-99`) — `median + rand01 * |
| `tests/game.rs:174` | `cOperationPurge.cpp:1190` | no move keys down ⇒ cruise (`cOperationPurge.cpp:1190`). |
| `tests/game.rs:433` | `—` | FrameChange returns before the game-over check in 2D (`:1229`). |

## Hardening guards (`GUARD`) — 3

| Rust `file:line` | C++ citation | Note |
|---|---|---|
| `src/error.rs:15` | `—` | replaces `cGameData::ReadFileXML`'s ignored `TiXmlDocument::LoadFile` |
| `src/error.rs:46` | `—` | the C++ performs zero header validation — a signed `cx`/`cy` pair |
| `src/error.rs:85` | `—` | building meshes keep 16-bit indices (design §1.3); a mesh |

## Corrected divergences (`FIX`) — 2

| Rust `file:line` | C++ citation | Note |
|---|---|---|
| `src/path/astar.rs:17` | `—` | the C++ used `g = squared distance` and `h = 3.0 * squared distance` |
| `src/path/mod.rs:238` | `cPathFinder.cpp:439-442` | the C++ computed the centered start/goal (`cPathFinder.cpp:439-442`) |

