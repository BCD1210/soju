# Soju Brood War: Blender asset assignment

The user explicitly requests true 3D VR in the original StarCraft: Brood War and visual quality matching StarCraft Remastered. They reject camera-facing sprites, 2.5D, box placeholders, and the current low-detail imported models. The user has asked to use their Dot for Blender asset work. This document is a concrete assignment, not evidence that a Dot has received or completed it.

## Scope

Create production-quality Marine and SCV assets in Blender first. Match the Remastered silhouette, armor construction, helmet/visor, hands/tools, feet, backpack, paint and surface detail. Compare against the user's locally installed Remastered art. Existing creator meshes are references and rig starting points; merely subdividing or sharpening their textures does not satisfy the request.

Deliver real geometry from front, sides, rear and above, with physically based materials, normal detail, clean UVs, and textures sufficient for an eye-level VR close-up. Start with a practical 15–40k triangle budget per hero-visible unit and supply a lower-detail version for groups; report measured cost. Preserve useful bone animation and produce idle, walk, attack and SCV work/carry clips. Keep geometry, not painted shadows, responsible for volume.

## Authorized project on Mike's Mac

Repository: /Users/mikekim/Projects/battlenet-macos
Runtime: /Users/mikekim/.local/share/soju/broodwar-native
Blender 4.5.15 LTS:
  runtime/blender/Blender.app/Contents/MacOS/Blender
Blender was downloaded from the official release server and SHA-256 checked.

Reference inputs:
- runtime/models-3d/marine.glb and scv.glb (rigged creator models)
- runtime/models-source/provenance.json and LICENSE
- runtime/scene-assets/image-239-*.dds and manifest.json: owned Remastered Marine
- runtime/scene-assets/image-247-*.dds and manifest.json: owned Remastered SCV
- runtime/3d-proof/: current front/side/rear renders showing the quality baseline
- runtime/asset-reference/: cropped reference frames, if present

External model source: TeamKoprulu/SCEvoComplete, pinned
773c8e8ae8828121e8f37bb74ab82ceb761a45bc. SCV KMC is a skin with the same geometry/UVs as the base SCBW SCV, but its gold palette is not the desired final quality/style. Source and provenance stay with the private assets; no Blizzard art is committed or publicly redistributed.

## Integration contract

The main coding agent owns the renderer and original game integration. Do not edit those files, stop game processes, change the Wine bottle, or overwrite currently installed models. Work only on the new assets and their source scripts.

Current glTF: right-handed, Y-up, creator forward +Z, foot-level pivot.
Current renderer scales Marine by 2 and SCV by 1.65; original source bounds are about 0.85 and 1.18 units high. Report any coordinate/scale changes explicitly.
Keep independently instantiable rigs and skinned meshes; avoid scene-global animation paths.
Clip names currently recognized: Stand_full, Walk_full, Attack_full, Work_full,
Stand Gather_full, Walk Gather_full. Supply an explicit mapping if names change.

Deliver into runtime/asset-upgrades:
- marine.blend and scv.blend, with packed textures and editable geometry
- marine.glb and scv.glb, each with embedded standard PNG/JPEG images
- six neutral-light views per model, plus close-up comparisons
- short animation turntables with foot contact and deformation visible
- validation.json: triangle counts, texture sizes, skeleton, clips, bounds, known defects

Review criteria: recognizably faithful Remastered design; full volume; no missing head/face;
correct normals/UVs; no floating feet; stable animation; clean front/side/back;
material quality in the actual real-time renderer, not only an offline beauty render.

The coding agent will review these deliverables, integrate the accepted GLBs, and re-run native movement/mining/production/construction/follow and OpenXR simulator controls. Physical-headset quality remains a separate unverified gate.
