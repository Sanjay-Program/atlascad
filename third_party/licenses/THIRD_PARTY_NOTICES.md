# Combined Third-Party Notices & Licenses

This document contains required legal notices and copyright disclosures for software components distributed with or utilized by AtlasCAD.

---

## 1. Apache License, Version 2.0
Components: `truck`, `Manifold`, `MuJoCo`

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

---

## 2. 3-Clause BSD License
Components: `pandapower`, `OpenDSS`, `Cantera`, `RDKit`, `EnergyPlus`, `OpenSees`

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:
1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.

---

## 3. GNU Lesser General Public License (LGPL v2.1 / v3.0)
Components: `Open CASCADE`, `SU2`, `IfcOpenShell`, `JSBSim`

These components are dynamically linked or orchestrated via standalone worker boundaries. Source code for modified upstream libraries is available via their upstream repositories listed in `third_party/MANIFEST.md`.

---

## 4. GNU General Public License (GPL v2.0 / v3.0)
Components: `CalculiX`, `OpenFOAM`, `Gmsh`, `KiCad`, `DWSIM`, `SolveSpace`

These components are executed as independent, process-isolated subprocesses via standard input/output streams and file exchange. They are not linked into AtlasCAD's core binary. Complete corresponding source code is available at their official repositories listed in `third_party/MANIFEST.md`.
