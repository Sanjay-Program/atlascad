# Competitor analysis

**Reviewed:** 2026-10-23. Prices below are vendor-posted US USD figures, before tax, and can vary by region, promotion, reseller, and checkout date. No India-specific quote was verified. Access dates are recorded per source in [`engineering/manifest/competitors.json`](../../engineering/manifest/competitors.json).

## Product snapshots

| Product | Price evidence | Product and deployment notes | License / restrictions |
|---|---|---|---|
| Autodesk Fusion | Autodesk currently lists Fusion at **$680/year** ($57/month billed annually) and $85 month-to-month on a personal-use comparison page. The public pages differ during promotions, so recheck checkout before any price comparison. | Integrated CAD/CAM/CAE/PCB and data management. Desktop application with cloud-connected project workflows; personal-use tier is limited and non-commercial. | Proprietary subscription; qualifying personal use is limited to home, non-commercial work and Autodesk's revenue/eligibility conditions. |
| AutoCAD | **$2,095/year** or **$260/month** for one US user per Autodesk's subscription FAQ; taxes excluded and local pricing may differ. | 2D drafting plus 3D solids, surfaces and meshes; specialized toolsets, APIs, web/mobile access, and cloud-connected review/publishing. | Proprietary subscription. AutoCAD LT is a separate 2D-focused product. |
| SOLIDWORKS Design | US single-user named-user plans: **Standard $2,820/year; Professional $3,456/year; Premium $4,716/year**, excluding local taxes. | Parts, assemblies, drawings, collaboration/data management; higher tiers add libraries, rendering, tolerance analysis, routing, and simulation/motion capabilities. Current offer is cloud-connected with a local install. | Proprietary, commercial plans. The official page lists a separate device-license option; regional pricing is at checkout/reseller. |
| Onshape | **Free $0** (non-commercial, all documents public); **Standard $1,500/user/year**; **Professional $2,500/user/year**; annual billing. | Browser/cloud CAD with real-time collaboration, version control and mobile applications. Paid tiers add private data and organization workflows. | Proprietary service. Free tier is non-commercial and documents are public; cloud service requires connectivity for ordinary operation. |
| FreeCAD | **$0**; no vendor subscription price. | Open-source, local, modular parametric 3D CAD with workbenches; TechDraw creates drawing pages and exports SVG/DXF/PDF. Capabilities depend on workbench and installed components. | LGPL-2.1-or-later for application code; third-party components and workbenches have their own terms. |
| SolveSpace | **$0**; no vendor subscription price. | Lightweight parametric 2D/3D CAD: constrained sketches, extrude/revolve/helix, Booleans, assemblies and several 2D/3D exports including DXF, STEP and STL. | GPLv3. The license matters for redistribution and is not a candidate for code reuse in a differently licensed shipping product without a separate review. |

## Workflow and product implications

The commercial products sell coherent downstream workflows, not just geometry: Fusion combines design and manufacturing; AutoCAD joins drafting, automation and sheet sets; SOLIDWORKS connects part/assembly/drawing work; Onshape makes versioning and collaboration part of the browser workflow. The open-source products demonstrate the value of local control, extensibility and a zero subscription price, while leaving usability, support and ecosystem consistency to the project and community.

Atlas should compete first on a dependable local workflow with a portable native project, predictable recompute, and clearly bounded export. The current repository can demonstrate a local browser workspace over the Rust engine: create a rectangle, circle or regular-polygon sketch, extrude it, edit named parameters, orbit and pick faces, undo/redo, save atomically, reopen, and export binary STL. It does **not** yet provide a native desktop window, sketch constraints, multiple features, booleans in the application, assemblies, drawings, STEP/DXF interchange, or cloud collaboration. The feature comparison records those limits explicitly.

## Evidence and confidence

Pricing and feature statements above are **verified vendor-published descriptions**, not independent performance evaluations. Feature scope is summarized at product-family level; packaging varies by tier. Prices are a point-in-time snapshot, not a quote or legal interpretation. User pain points are kept separate and sourced to public community discussions in [`user-pain-points.md`](user-pain-points.md).

Primary sources:

- [Autodesk Fusion pricing and personal-use comparison](https://www.autodesk.com/products/fusion-360/personal)
- [Autodesk Fusion plan overview](https://www.autodesk.com/products/fusion-360/overview)
- [Autodesk AutoCAD subscription FAQ](https://www.autodesk.com/solutions/autocad-subscription-faq)
- [Autodesk AutoCAD feature overview](https://www.autodesk.com/products/autocad/features)
- [SOLIDWORKS Design plans and pricing](https://www.solidworks.com/how-to-buy/solidworks-design-plans-pricing)
- [SOLIDWORKS Design capability matrix](https://www.solidworks.com/product/solidworks-design)
- [Onshape plans and pricing](https://www.onshape.com/en/pricing)
- [FreeCAD official source and license](https://github.com/FreeCAD/FreeCAD)
- [FreeCAD official TechDraw documentation](https://github.com/FreeCAD/FreeCAD-documentation/blob/main/wiki/Drawing.md)
- [SolveSpace official features](https://solvespace.com/features.pl)
- [SolveSpace official project overview](https://solvespace.com/index.pl)
