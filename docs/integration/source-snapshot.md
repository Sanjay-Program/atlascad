# Imported source snapshot

This document records the upstream source locations and exact local revisions included in the consolidated AtlasCAD workspace snapshot. The CADCraft and SolidCraft entries include the local changes committed immediately before consolidation. All other entries are the checked-out upstream revisions as found in the workspace.

| Path | Origin | Included revision |
| --- | --- | --- |
| `apps/cadcraft` | <https://github.com/storytold/cadcraft> | `8630d6b6928cc7ea5387d1f2adccc169a5ae74dd` |
| `apps/solidcraft` | <https://github.com/tangency-labs/solidcraft> | `5ad95cbd30585b46aa77004999c5673cae35ab0b` |
| `kernels/truck` | <https://github.com/ricosjp/truck> | `78d699b0792b39e27c2bf073b682986701644e91` |
| `kernels/monstertruck` | <https://github.com/virtualritz/monstertruck> | `d2ad1ed7aa420b7f32cd2c1e3ad319b46d593652` |
| `kernels/cascade-rs` | <https://github.com/DonSqualo/cascade-rs> | `c68c1d2c3c7ff24d9f0d21b926748273bb856842` |
| `kernels/hagane` | <https://github.com/rsasaki0109/Hagane> | `45e423c9c5f1eea33a5ca23e44abde2d63cdd95d` |
| `kernels/cadrum` | <https://github.com/lzpel/cadrum> | `c59916dbbf135c21c64d80f378b0674432df50bb` |
| `kernels/opencascade-rs` | <https://github.com/bschwind/opencascade-rs> | `32758df23a137f4e7c786c618b0317fc3badb3b2` |
| `references/cadrs` | <https://github.com/gswdh/cadrs> | `74dd5771cfc3f3c2c75958d273aa032d92d1e1f2` |
| `references/cadrust` | <https://github.com/mac999/cadrust> | `519cb390fd1c2e3079dd520ac4899474565f6b98` |
| `references/bevy_pmetra` | <https://github.com/nilaysavant/bevy_pmetra> | `6933b23d75bd3168ab398bb288a40a6cd3f2c516` |

The snapshot preserves source files, copyright notices, license texts, and project documentation. It does not import the original Git commit histories. The new repository's history begins with the dated import commit; that commit does not claim authorship of upstream work.

`OpenCADStudio-main.zip` is not part of the snapshot. The archive was identified as GPL-3.0 and remains excluded by the root `.gitignore`; no source from it was copied into this repository.
