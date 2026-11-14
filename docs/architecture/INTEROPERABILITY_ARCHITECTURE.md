# AtlasCAD Interoperability & Data Exchange Architecture

## Overview
Engineering workflows cross disciplinary and organizational boundaries. AtlasCAD provides a centralized, standards-compliant format management engine supporting industry-standard exchange formats without proprietary lock-in.

---

## 1. Supported Format Matrix

| Domain | Standard Format | Specification Version | Import Support | Export Support | Underlying Adapter |
| :--- | :--- | :--- | :---: | :---: | :--- |
| **Mechanical CAD** | **STEP** | ISO 10303 AP203 / AP214 / AP242 | Yes | Yes | OCCT Bridge (`cad-io`) |
| **Mechanical CAD** | **IGES** | Initial Graphics Exchange 5.3 | Yes | Yes | OCCT Bridge (`cad-io`) |
| **Mechanical CAD** | **STL** | Binary / ASCII Stereolithography | Yes | Yes | Native Rust (`cad-io`) |
| **Mechanical CAD** | **OBJ / glTF** | Wavefront / Khronos glTF 2.0 | Yes | Yes | Native Rust (`cad-contract`) |
| **Drafting** | **DXF** | AutoCAD R12 ASCII DXF | Yes | Yes | Native Rust (`cad-drafting`) |
| **Drafting** | **PDF** | ISO 32000 Vector PDF | — | Yes | Native Rust (`cad-drafting`) |
| **Architecture / BIM** | **IFC** | buildingSMART IFC2x3 / IFC4 | Yes | Yes | IfcOpenShell Adapter |
| **Electronics / ECAD** | **SPICE Netlist** | Standard Berkeley SPICE 3f5 | Yes | Yes | ngspice Adapter |
| **Electronics / ECAD** | **Gerber / Excellon**| RS-274X Extended Gerber | Yes | Yes | KiCad CLI Adapter |
| **Scientific / Simulation**| **VTK / VTU** | Visualization Toolkit XML Unstructured | Yes | Yes | Native Rust Serializer |
| **Geospatial / GIS** | **GeoJSON / Shapefile**| RFC 7946 GeoJSON / ESRI Shapefile | Yes | Yes | GDAL Adapter |
| **Water Hydraulics** | **EPANET INP** | EPA Water Distribution Network | Yes | Yes | EPANET Adapter |

---

## 2. Robust Parsing & Translation Architecture
- **Fault-Tolerant Geometry Healing**: Automatic detection and repair of unstitched seams, inverted surface normals, and duplicate vertices during STEP/IGES import.
- **Unit Normalization**: Automatic detection and conversion across millimeters, meters, inches, and feet into internal standard SI millimeter/meter units.
- **Non-Destructive Round-Tripping**: Imported geometry preserves upstream entity IDs and layer assignments.
