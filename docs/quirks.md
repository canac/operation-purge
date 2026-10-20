# Parity ledger (quirks registry)

> **Generated file — do not edit by hand.** Regenerate with
> `./scripts/gen_quirks.sh` after adding or moving any
> `// PARITY:` / `// GUARD:` / `// FIX:` comment.

Every intentional divergence from — or bug-for-bug reproduction of —
the 2011 C++/Direct3D9 original is pinned in the source with one of
three tags, each citing the C++ ground truth by `file:line`
(`reference/OperationPurge`). This table is that ledger,
harvested straight from those comments.

| Tag | Meaning |
|---|---|
| `PARITY` | Behaviour deliberately reproduced bug-for-bug from the C++. |
| `GUARD`  | A hardening/validation check the original lacked. |
| `FIX`    | A corrected divergence — a C++ bug **not** reproduced. |

**187** ledger entries found across `src tests`.

## Parity quirks (`PARITY`) — 176

| Rust `file:line` | C++ citation | Note |
|---|---|---|
| `src/app/mod.rs:273` | `—` | the cursor shows only on the 2D map / menus (`ShowCursor`, |
| `src/audio/mod.rs:24` | `—` | `IDirectSoundBuffer::SetVolume` takes hundredths of a decibel |
| `src/audio/mod.rs:211` | `—` | the steal decision compares distances from `live_listener`, |
| `src/audio/spatial.rs:27` | `—` | reproduces the DS3D inverse-distance model with `flMinDistance |
| `src/audio/spatial.rs:47` | `—` | DirectSound uses a **left-handed** coordinate system, so the |
| `src/audio/spatial.rs:74` | `—` | `ListenerData` in `cOperationPurge::ExecuteCommands` |
| `src/audio/spatial.rs:166` | `—` | DS3D is left-handed. Facing +Y, right = Z × Y = -X, so a |
| `src/data/builtin.rs:15` | `cOperationPurge.cpp:2153-2199` | verified 11-value record, `cOperationPurge.cpp:2153-2199` (+ |
| `src/data/mod.rs:167` | `—` | unused for turning (rate-limit code commented out, |
| `src/data/mod.rs:190` | `—` | only consumed by dead code (`cObstaclePathFinder`, not |
| `src/data/mod.rs:221` | `—` | an ammo center with no entry for a fired projectile type has |
| `src/data/mod.rs:317` | `—` | a *delay* between emits (seconds, or distance units when |
| `src/data/values.rs:17` | `—` | alpha is forced opaque here for every `[colors]` field except |
| `src/game/mod.rs:25` | `—` | game speed starts paused at `0.0` after robot selection |
| `src/game/mod.rs:31` | `—` | the enemy-visible HUD/alarm poll runs every 0.25 s of *game* time |
| `src/game/mod.rs:35` | `—` | `MIN_VISIBLE_SIZE` — the smallest 2D-map window, in rooms |
| `src/game/mod.rs:44` | `—` | `cOperationPurge::eObserverType` — which vantage the 3D camera + |
| `src/game/mod.rs:166` | `cOperationPurge.cpp:144-147` | `SetGameParams` shows the whole building (`cOperationPurge.cpp:144-147`). |
| `src/game/mod.rs:184` | `—` | the view-window update at the top of `Render2D` |
| `src/game/mod.rs:209` | `cOperationPurge.cpp:3148-3163` | `cOperationPurge::WinToLog` (`cOperationPurge.cpp:3148-3163`) — |
| `src/game/mod.rs:252` | `cOperationPurge.cpp:1229-1233` | the sim is frozen in the 2D map (`cOperationPurge.cpp:1229-1233`) |
| `src/game/mod.rs:266` | `—` | `m_bForceChange` — set by `Enter2DMode(true)` when the active |
| `src/game/mod.rs:280` | `cOperationPurge.cpp:2037-2061` | `cOperationPurge::Enter2DMode` (`cOperationPurge.cpp:2037-2061`). |
| `src/game/mod.rs:300` | `cOperationPurge.cpp:2015-2034` | `cOperationPurge::GetRobotUnderPoint` (`cOperationPurge.cpp:2015-2034`) |
| `src/game/mod.rs:312` | `cOperationPurge.cpp:957-966` | `commandSelectObserverRobot` handler (`cOperationPurge.cpp:957-966`) |
| `src/game/mod.rs:331` | `cOperationPurge.cpp:968-977` | `commandSelectObserverPosition` handler (`cOperationPurge.cpp:968-977`). |
| `src/game/mod.rs:340` | `cOperationPurge.cpp:979-1018` | `commandSetObserver` handler (`cOperationPurge.cpp:979-1018`). |
| `src/game/mod.rs:363` | `cOperationPurge.cpp:1020-1035` | `commandExit2DMode` handler (`cOperationPurge.cpp:1020-1035`). |
| `src/game/mod.rs:425` | `—` | `cOperationPurge::StartGame` restart path |
| `src/game/mod.rs:462` | `cOperationPurge.cpp:880-1225` | `cOperationPurge::ExecuteCommands` (`cOperationPurge.cpp:880-1225`). |
| `src/game/mod.rs:481` | `—` | EndGame (Esc) / Restart (F4) are tested before the 2D-mode |
| `src/game/mod.rs:566` | `—` | no upper bound; only clamped below at 0 (`:1145`). |
| `src/game/mod.rs:636` | `cOperationPurge.cpp:1064-1120` | the free-observer command block (`cOperationPurge.cpp:1064-1120` |
| `src/game/mod.rs:739` | `cOperationPurge.cpp:1227-1420` | `cOperationPurge::FrameChange` (`cOperationPurge.cpp:1227-1420`). |
| `src/game/mod.rs:784` | `—` | the win/lose/draw check at the END of `FrameChange` |
| `src/game/mod.rs:801` | `—` | the targeted / above-center / enemy-visible flags computed in |
| `src/game/mod.rs:840` | `cOperationPurge.cpp:1596-1634` | `GetVisibleRobots` (`cOperationPurge.cpp:1596-1634`) restricted |
| `src/game/mod.rs:970` | `cOperationPurge.cpp:3034-3056` | `cOperationPurge::SetupScene` view pose (`cOperationPurge.cpp:3034-3056`) |
| `src/game/newgame.rs:21` | `—` | an empty password string enables cheats |
| `src/game/newgame.rs:28` | `cMainWindow.cpp:758` | `szPassword[40]` (`cMainWindow.cpp:758`) — the Win32 edit control |
| `src/game/newgame.rs:74` | `—` | Cancel behaviour differs by whether a game is already running: |
| `src/game/newgame.rs:113` | `cMainWindow.cpp:760-761` | empty password ⇒ cheats on (`cMainWindow.cpp:760-761`). |
| `src/game/screens.rs:24` | `—` | `cOperationPurge::ExecuteCommands` game-over message text |
| `src/input/commands.rs:7` | `—` | variant order matches `cGameData::s_aszCommandsNames` |
| `src/input/commands.rs:118` | `—` | transcribed from the original `GameData.xml` `fire_command`s. |
| `src/input/commands.rs:170` | `cGameData.cpp:17-54` | pins the eCommand ABI (cGameData.cpp:17-54). |
| `src/input/commands.rs:186` | `—` | bExactModifiers = false — a command with no required |
| `src/math/d3dx.rs:237` | `—` | NaN inputs propagate — `norm` |
| `src/math/d3dx.rs:496` | `—` | NaN in -> NaN out (no finiteness guard); the "NaN on zero |
| `src/math/intersect.rs:41` | `Intersect.h:107` | `Intersect.h:107,115` — literally `1.0`, the same value as a |
| `src/math/intersect.rs:983` | `—` | a line that passes fully through a symmetric box (both |
| `src/math/mod.rs:12` | `Functions.cpp:8` | `Functions.cpp:8` `Equals` uses an **absolute** epsilon of |
| `src/math/mod.rs:21` | `Functions.cpp:83-86` | `Functions.cpp:83-86` — `fabsf (fNumber1 - fNumber2) <= COMPARE_TOLERANCE` |
| `src/math/mod.rs:67` | `Functions.cpp:56-62` | `Functions.cpp:56-62` `Between` — inclusive range test in either |
| `src/math/mod.rs:80` | `Functions.cpp:78-81` | `Functions.cpp:78-81` `Sign` — `fNumber >= 0.0f ? 1.0f : -1.0f` |
| `src/math/mod.rs:92` | `Functions.cpp:64-75` | `Functions.cpp:64-75` `Floor` — a truncation-based `floor` that |
| `src/math/mod.rs:132` | `Functions.cpp:191-200` | `Functions.cpp:191-200` `GetAngle(pt2Point1, pt2Point2)` = |
| `src/math/mod.rs:146` | `Functions.cpp:157-164` | `Functions.cpp:157-164` `SphereCoord` — |
| `src/math/mod.rs:164` | `Functions.cpp:166-174` | `Functions.cpp:166-174` `AcceleratePoint(fTime, pt3Position, |
| `src/math/mod.rs:189` | `Functions.cpp:83-86` | Functions.cpp:83-86 uses `<=`, so a difference of exactly |
| `src/maze/building.rs:543` | `cBuilding.cpp:164-208` | `cBuilding::IntersectRect` (`cBuilding.cpp:164-208`) — static |
| `src/maze/building.rs:578` | `—` | `cBuilding::IntersectMovingRect` -> `GetIntersectedWalls` |
| `src/maze/building.rs:588` | `cBuilding.cpp:219-273` | `cBuilding::IntersectTrajectory` (`cBuilding.cpp:219-273`). |
| `src/maze/building.rs:637` | `—` | `cBuilding::GetIntersectedWallsPortion` |
| `src/maze/mod.rs:64` | `cMaze.cpp:179-204` | `cMaze::OpenMaze` (`cMaze.cpp:179-204`) reads the 8-byte |
| `src/maze/search.rs:25` | `—` | the `strchr` anchor jump is a raw byte search — it is **not** |
| `src/path/astar.rs:202` | `cPathFinder.cpp:294` | `cPathFinder.cpp:294` — only skip if the node is |
| `src/path/astar.rs:230` | `cPathFinder.cpp:225-237` | `cPathFinder.cpp:225-237` — walk parents back to the |
| `src/path/mod.rs:92` | `cPathFinder.cpp:373-401` | `cPathFinder::IsPointVisible` (`cPathFinder.cpp:373-401`). |
| `src/path/mod.rs:116` | `cPathFinder.cpp:454-480` | `cHighLevelPathFinder::GetNeighbors` (`cPathFinder.cpp:454-480`). |
| `src/path/mod.rs:141` | `cPathFinder.cpp:521-595` | `cLowLevelPathFinder::GetNeighbors` (`cPathFinder.cpp:521-595`). |
| `src/path/mod.rs:239` | `cPathFinder.cpp:501-518` | `cLowLevelPathFinder::FindPath` (`cPathFinder.cpp:501-518`). |
| `src/path/nodes.rs:28` | `cPathFinder.cpp:14` | `cPathFinder.cpp:14` `s_nNodesPerPoint = 256` — the hash |
| `src/path/nodes.rs:66` | `cNodeList.cpp:19-23` | `cNodeList::Reset` (`cNodeList.cpp:19-23`) — clears the |
| `src/path/nodes.rs:79` | `—` | `cNodeList::HashPoint` |
| `src/profiler.rs:103` | `—` | `cProfileNode::GetSubNode` linear-scans siblings by name. |
| `src/render/mesh.rs:30` | `—` | normals are stored and uploaded exactly as computed — never |
| `src/render/overlay.rs:268` | `—` | `cOverlay2D::StartRender` visibility bounds — whole rooms fully |
| `src/render/overlay.rs:374` | `cOperationPurge.cpp:2762-2835` | the building half of `Render2D` (`cOperationPurge.cpp:2762-2835`): |
| `src/render/overlay.rs:414` | `—` | `cOverlay2D::RenderCrosshair` — screen-centered, 3D mode only. |
| `src/render/robot3d.rs:54` | `cRobot3D.cpp:188` | z compressed by fBodyHeight/fRadius (cRobot3D.cpp:188). |
| `src/render/robot3d.rs:56` | `—` | normal = the (un-normalized) scaled position. |
| `src/render/robot3d.rs:93` | `—` | un-normalized. |
| `src/render/scene.rs:129` | `—` | dead robots and the active robot (the eye sits inside it) are |
| `src/render/scene.rs:393` | `cOperationPurge.cpp:2866-3030` | `Render3D` order (`cOperationPurge.cpp:2866-3030`), all D3DCULL_CCW |
| `src/render/shaders.rs:51` | `—` | FFP Gouraud lighting, one white point light AT THE EYE. |
| `src/render/shaders.rs:54` | `—` | N_view = mat3(world_view) * N, NO inverse-transpose, N NOT normalized. |
| `src/rng.rs:26` | `—` | `fRand() = rand() / (RAND_MAX + 1)` — a value in `[0, 1)` |
| `src/sim/ai.rs:129` | `—` | two `fRand ()` draws in this order — the scan delay inside |
| `src/sim/ai.rs:221` | `cOperationPurge.cpp:1691-1728` | `IsRobotVisible`/`IsCenterVisible` (`cOperationPurge.cpp:1691-1728`): |
| `src/sim/ai.rs:270` | `cAIRobot.cpp:64-98` | `cAIRobot::CalcNewPositionAI` (`cAIRobot.cpp:64-98`). Returns |
| `src/sim/ai.rs:423` | `cAIRobot.cpp:410-426` | brake law v²/(2d) vs friction/brake decel (cAIRobot.cpp:410-426). |
| `src/sim/ai.rs:472` | `cRobot.cpp:336-370` | `cRobot::Fire (const cRobot&)` (`cRobot.cpp:336-370`): solve |
| `src/sim/ai.rs:549` | `cAIRobot.cpp:102-175` | `cAIRobot::MoveAI` (`cAIRobot.cpp:102-175`): bounce off |
| `src/sim/ai.rs:565` | `cAIRobot.cpp:150-155` | logged and bailed before `cRobot::Move` (`cAIRobot.cpp:150-155`). |
| `src/sim/ai.rs:583` | `cAIRobot.cpp:545-620` | `cAIRobot::ChangeDesiredDirection` (`cAIRobot.cpp:545-620`). |
| `src/sim/ai.rs:637` | `cAIRobot.cpp:135-148` | the wall-bounce heading snap (`cAIRobot.cpp:135-148`). A `planeX` |
| `src/sim/center.rs:107` | `cCenter.cpp:78-91` | `cCenter::IsWithinBoundries` (`cCenter.cpp:78-91`) — the |
| `src/sim/center.rs:125` | `cCenter.cpp:99-104` | `cCenter::IsTransferPossible` (`cCenter.cpp:99-104`) — the |
| `src/sim/idgen.rs:14` | `cIDGenerator.cpp:5` | `cIDGenerator.cpp:5` — both ternary branches evaluate to this, |
| `src/sim/idgen.rs:34` | `cIDGenerator.cpp:18-31` | `cIDGenerator.cpp:18-31` `Next()` verbatim: |
| `src/sim/idgen.rs:67` | `cIDGenerator.cpp:34-38` | `cIDGenerator.cpp:34-38` `Reset()`. |
| `src/sim/particles.rs:55` | `cParticle.cpp:10-27` | `cParticle::cParticle` (`cParticle.cpp:10-27`). Constructor |
| `src/sim/particles.rs:86` | `cParticle.cpp:52-57` | `cParticle::Move` (`cParticle.cpp:52-57`). |
| `src/sim/particles.rs:93` | `cParticle.cpp:60-63` | `cParticle::IsLifeExpired` (`cParticle.cpp:60-63`). |
| `src/sim/particles.rs:98` | `cParticle.cpp:66-69` | `cParticle::GetLifetimePortion` (`cParticle.cpp:66-69`). |
| `src/sim/particles.rs:105` | `cParticle.cpp:88-121` | `cParticle::GetStage` (`cParticle.cpp:88-121`). Returns |
| `src/sim/particles.rs:126` | `cParticle.cpp:124-135` | `cParticle::GetSize` (`cParticle.cpp:124-135`). |
| `src/sim/particles.rs:135` | `cParticle.cpp:138-141` | `cParticle::GetRotation` (`cParticle.cpp:138-141`). |
| `src/sim/particles.rs:140` | `cParticle.cpp:144-155` | `cParticle::GetColor` (`cParticle.cpp:144-155`) — full RGBA |
| `src/sim/particles.rs:178` | `cOperationPurge.cpp:1454-1483` | `cOperationPurge::AddParticle` (`cOperationPurge.cpp:1454-1483`). |
| `src/sim/particles.rs:209` | `—` | the "Move particles" pass of `MoveObjects` |
| `src/sim/particles.rs:281` | `—` | the shortest-arc rotation that maps +Z onto `emitter_velocity`, |
| `src/sim/particles.rs:324` | `cParticleEmitter.cpp:10-24` | `cParticleEmitter::cParticleEmitter` (`cParticleEmitter.cpp:10-24`). |
| `src/sim/particles.rs:352` | `cParticleEmitter.cpp:79-82` | `cParticleEmitter::IsLifeExpired` (`cParticleEmitter.cpp:79-82`). |
| `src/sim/particles.rs:357` | `cParticleEmitter.cpp:130-133` | `GetEmitDelay` (`cParticleEmitter.cpp:130-133`). |
| `src/sim/particles.rs:363` | `—` | `cParticleEmitter::GetEmitVelocity(vec3Emitter)` |
| `src/sim/particles.rs:384` | `—` | `cParticleEmitter::Move(fTimeElapsed)` |
| `src/sim/particles.rs:393` | `—` | `cParticleEmitter::Move(fTimeElapsed, pt3Position, vec3Velocity)` |
| `src/sim/particles.rs:419` | `—` | fEmitRate is a delay, consumed by sub-frame interpolation. |
| `src/sim/particles.rs:461` | `cParticleEffect.cpp:6-27` | `cParticleEffect::cParticleEffect` (`cParticleEffect.cpp:6-27`). |
| `src/sim/particles.rs:499` | `cParticleEffect.cpp:40-49` | `cParticleEffect::Move(fTimeElapsed)` (`cParticleEffect.cpp:40-49`). |
| `src/sim/particles.rs:507` | `—` | `cParticleEffect::Move(fTimeElapsed, pt3Position, vec3Velocity)` |
| `src/sim/particles.rs:525` | `cParticleEffect.cpp:65-68` | `cParticleEffect::IsFinished` (`cParticleEffect.cpp:65-68`). |
| `src/sim/particles.rs:530` | `cParticleEffect.cpp:71-97` | `cParticleEffect::RemoveEmitters` (`cParticleEffect.cpp:71-97`). |
| `src/sim/particles.rs:556` | `—` | D3DXVec3Normalize(0) -> NaN, no fallback fires. |
| `src/sim/particles.rs:563` | `—` | velocity along -Z -> w normalizes to 0 -> (0,0,1,0) |
| `src/sim/projectile.rs:90` | `cProjectile.cpp:108-135` | `cProjectile::Move` (`cProjectile.cpp:108-135`). Advances one |
| `src/sim/projectile.rs:110` | `cProjectile.cpp:139-157` | `cProjectile::IntersectRobot` (`cProjectile.cpp:139-157`). |
| `src/sim/projectile.rs:124` | `cProjectile.cpp:161-177` | `cProjectile::IntersectBuilding` (`cProjectile.cpp:161-177`). |
| `src/sim/projectile.rs:138` | `cProjectile.cpp:180-187` | `cProjectile::AddCollision` (`cProjectile.cpp:180-187`) — |
| `src/sim/projectile.rs:151` | `cProjectile.cpp:229-233` | `cProjectile::GetDamage` (`cProjectile.cpp:229-233`): |
| `src/sim/projectile.rs:158` | `cProjectile.cpp:321-357` | `cProjectile::DisplayCollisionEffect` (`cProjectile.cpp:321-357`). |
| `src/sim/projectile.rs:179` | `—` | `cProjectile::DisplayHole` position/orientation |
| `src/sim/projectile.rs:227` | `—` | `cProjectile::GetTrajectory` (the static aim solver, |
| `src/sim/robot.rs:25` | `—` | `Make2D (cRobot3D::GetSize ())` = `(fBarrelLength*2, fBarrelLength*2)` |
| `src/sim/robot.rs:30` | `—` | robot 3D box height `fBodyHeight + fTreadRadius*2` |
| `src/sim/robot.rs:34` | `cRobot3D.cpp:40` | `cRobot3D::GetCenter ()` = `(0, 0, size.z * 0.5)` (`cRobot3D.cpp:40`). |
| `src/sim/robot.rs:61` | `cRobot.cpp:838-841` | `cRobot::tProjectileTypeState::FireReady` (`cRobot.cpp:838-841`) |
| `src/sim/robot.rs:318` | `cRobot.cpp:791-835` | `cRobot::GetMoveAccel` (`cRobot.cpp:791-835`). Returns |
| `src/sim/robot.rs:345` | `cRobot.cpp:94-136` | `cRobot::CalcNewPosition` (`cRobot.cpp:94-136`). Phase 1 — |
| `src/sim/robot.rs:365` | `cRobot.cpp:117-120` | instantaneous turn (rate-limit commented out, cRobot.cpp:117-120). |
| `src/sim/robot.rs:379` | `cRobot.cpp:226-275` | `cRobot::AddCollision` (`cRobot.cpp:226-275`). Records the |
| `src/sim/robot.rs:439` | `cRobot.cpp:145-223` | `cRobot::Move` (`cRobot.cpp:145-223`). Phase 3 — commits the |
| `src/sim/robot.rs:461` | `cRobot.cpp:488-513` | `cRobot::IntersectRobot` (`cRobot.cpp:488-513`). |
| `src/sim/robot.rs:498` | `cRobot.cpp:427-441` | `cRobot::GetProjectileFireType` (`cRobot.cpp:427-441`) — the |
| `src/sim/robot.rs:510` | `cRobot.cpp:444-458` | `cRobot::GetProjectileRestockType` (`cRobot.cpp:444-458`) — the |
| `src/sim/robot.rs:522` | `cRobot.cpp:279-313` | `cRobot::Fire (tProjectileTypeID)` (`cRobot.cpp:279-313`). |
| `src/sim/robot.rs:564` | `cRobot.cpp:461-471` | `cRobot::IncreaseAmmo` (`cRobot.cpp:461-471`) — clamped to the |
| `src/sim/robot.rs:595` | `—` | `cRobot::Fire (const cRobot&)` fire-alignment gate |
| `src/sim/robot.rs:603` | `cRobot3D.cpp:45-49` | `cRobot3D::GetFiringOffset` (`cRobot3D.cpp:45-49`): |
| `src/sim/world.rs:60` | `cOperationPurge.cpp:1190` | no move keys down => cruise (`cOperationPurge.cpp:1190`). |
| `src/sim/world.rs:183` | `cOperationPurge.cpp:153-306` | `cOperationPurge::StartGame` (`cOperationPurge.cpp:153-306`). |
| `src/sim/world.rs:206` | `—` | initial spawn point uses the robot's 3D footprint |
| `src/sim/world.rs:346` | `cAIRobot.cpp:56-61` | `cAIRobot::Reset` (`cAIRobot.cpp:56-61`) on robot `id`, which |
| `src/sim/world.rs:406` | `cOperationPurge.cpp:1441-1450` | `cOperationPurge::AddProjectile` (`cOperationPurge.cpp:1441-1450`) |
| `src/sim/world.rs:430` | `—` | the travel effect is created in the `cProjectile` ctor, |
| `src/sim/world.rs:450` | `cOperationPurge.cpp:804-865` | `cOperationPurge::TestCollisions` (`cOperationPurge.cpp:804-865`) |
| `src/sim/world.rs:466` | `—` | security break against an infinite loop |
| `src/sim/world.rs:492` | `—` | `cOperationPurge::IntersectRobotObjects` |
| `src/sim/world.rs:521` | `—` | the "Move projectiles" pass of `MoveObjects` |
| `src/sim/world.rs:605` | `—` | `cOperationPurge::RemoveDestroyedRobots` |
| `src/sim/world.rs:622` | `cRobot.cpp:419-423` | `cRobot::DisplayDestroyEffect` (`cRobot.cpp:419-423`) |
| `src/sim/world.rs:630` | `cOperationPurge.cpp:868-877` | `cOperationPurge::Transfer` (`cOperationPurge.cpp:868-877`) |
| `src/sim/world.rs:638` | `—` | `cCenter::ChooseNewTransferRobot`/`GetTransferRobot` + |
| `src/sim/world.rs:718` | `—` | an unlisted type transfers at rate 0.0, so `te / 0.0` |
| `src/sim/world.rs:737` | `—` | `cOperationPurge::GetNewTransferRobot` |
| `src/sim/world.rs:752` | `cOperationPurge.cpp:445-516` | `cOperationPurge::GetGameStatus` (`cOperationPurge.cpp:445-516`). |
| `src/sim/world.rs:797` | `cOperationPurge.cpp:2235-2309` | `cOperationPurge::SpawnRobots` (`cOperationPurge.cpp:2235-2309`). |
| `src/sim/world.rs:873` | `cOperationPurge.cpp:2347-2369` | `cOperationPurge::GetRectPosition` (`cOperationPurge.cpp:2347-2369`) |
| `src/sim/world.rs:896` | `—` | `cOperationPurge::IntersectRectObjects` |
| `src/sim/world.rs:908` | `—` | `cOperationPurge::InitializeCenterPositions` |
| `src/sim/world.rs:956` | `—` | `cOperationPurge::CreateCenterGroups` |
| `src/sim/world.rs:1023` | `—` | `cOperationPurge::GetCenterPosition` |
| `tests/game.rs:173` | `cOperationPurge.cpp:1190` | no move keys down ⇒ cruise (`cOperationPurge.cpp:1190`). |
| `tests/game.rs:477` | `—` | FrameChange returns before the game-over check in 2D (`:1229`). |

## Hardening guards (`GUARD`) — 8

| Rust `file:line` | C++ citation | Note |
|---|---|---|
| `src/data/gamedata.rs:53` | `—` | the original never validated `*Ref` indices and would index out |
| `src/error.rs:14` | `—` | replaces `cGameData::ReadFileXML`'s ignored `TiXmlDocument::LoadFile` |
| `src/error.rs:47` | `—` | the C++ performs zero header validation — a signed `cx`/`cy` pair |
| `src/error.rs:85` | `—` | building meshes keep 16-bit indices; a mesh that would |
| `src/game/newgame.rs:100` | `—` | pull persisted indices into range for `data`; the original's |
| `src/maze/mod.rs:18` | `—` | the original does zero header validation — a |
| `src/maze/mod.rs:83` | `—` | the |
| `src/maze/mod.rs:206` | `—` | out-of-grid queries return "no wall" rather |

## Corrected divergences (`FIX`) — 3

| Rust `file:line` | C++ citation | Note |
|---|---|---|
| `src/game/mod.rs:483` | `—` | on the press edge, not `bDown`: the blocking MessageBox ate the |
| `src/path/astar.rs:16` | `—` | the C++ used `g = squared distance` and `h = 3.0 * squared distance` |
| `src/path/mod.rs:220` | `cPathFinder.cpp:439-442` | the C++ computed the centered start/goal (`cPathFinder.cpp:439-442`) |

