# Localization parity

> **Last reviewed:** 2026-10-23 · **Last updated:** 2026-10-23 · **Change:** major (recounted every checked-in catalog and measured missing Ukrainian-reference keys; corrected stale language/status claims) · **Target:** Autodesk AutoCAD 2027 for Mac (26.0)

How far CADCraft's interface and drawing text are from AutoCAD's languages. Summarised in
[ROADMAP.md](../ROADMAP.md#languages); work items in [gaps.md](gaps.md#localization).

## What the target ships

Measured from the installed AutoCAD 2027 for Mac bundle by listing only (folder names, nothing
read): `Contents/Resources` has `Base.lproj` (English), `de`, `es`, `fr`, `it`, `ja`, `ko` and
`zh_CN` localizations, and `Resources/Support` has matching `@de@` … `@zh_CN@` folders. That is
**8 interface languages** on the Mac. AutoCAD for Windows also has language packs for Traditional
Chinese, Brazilian Portuguese, Russian, Polish, Czech and Hungarian (Autodesk's published
language-pack list, not measured here). AutoCAD's spell checker ships about 20 dictionaries
(`*.dct` names in `Resources/Support`).

Drawing text in AutoCAD: TrueType and SHX big fonts (CJK), Unicode `\U+` escapes, complex-script
shaping and right-to-left text through TrueType, vertical SHX text styles.

## What CADCraft has

- **A display-time string catalog** (`crates/ui-egui/src/i18n/`, landed with #36): a TSV per
  language (context, English source, translation, `@plural` rows), Edit → Interface Language with
  Auto (system language), live switching, a saved choice, and tests for coverage, placeholders
  and plurals ([localization.md](localization.md)). The Ukrainian catalog has **1,350 rows**;
  German has **1,031**. The 15 non-English catalogs have differing measured coverage of menus,
  command labels, tool groups and tooltips, the Start page, tabs, status controls, object snaps,
  drafting prompts, property labels and dialogs. Engine diagnostics,
  CLI/MCP responses and command history stay English, and uncatalogued text falls back to
  English. For scale: about 1,160 unique capitalised string literals exist in `crates/engine`
  and `crates/ui-egui` (grep, 2026-10-10).
- **Drawing text:** TrueType/OpenType/`.ttc` through `skrifa` with per-character font fallback
  and FONTALT, so CJK characters render (#173). DXF text is written with `\U+` escapes so
  non-ASCII survives in R2000 files (#169). Lines with right-to-left text get the Unicode
  Bidirectional Algorithm and OpenType shaping with `harfrust` (Arabic, Persian and Urdu joining,
  lam-alef, ZWNJ, marks; Hebrew), in TEXT and MTEXT including wrapping (#287). Left-to-right lines
  are still placed one glyph per code point: **no shaping** for Devanagari and other Indic
  scripts, and **no vertical text**. SHX fonts are not read; our stroke font stands in.
- **Text input:** egui/winit IME. Several upstream IME fixes are queued as PRs (#110–#119:
  macOS Korean, emoji picker, Windows caret placement, X11 and Wayland), so CJK input is not yet
  reliable on every platform.

## Languages

The following counts were measured from the checked-in UTF-8 TSV files on 2026-10-23. “Rows”
counts catalog entries, including plural templates. “Missing reference keys” compares each file’s
(context, English source) keys with the Ukrainian reference catalog (1,350 keys); it is not a
percentage of every runtime string. Missing catalog entries fall back to English. UI coverage tests
also report gaps for the menu/command/tool/snap sources and marked UI literals.

| Language | Code | Catalog rows | Missing Ukrainian-reference keys | Current status |
|---|---|---:|---:|---|
| English | en | source | — | source language |
| Arabic | ar | 1,229 | 127 | partial; drawing text has RTL shaping, UI layout is not mirrored |
| Brazilian Portuguese | pt-BR | 1,243 | 111 | partial |
| Czech | cs | 1,215 | 143 | partial; native proofreading remains pending |
| German | de | 1,031 | 319 | partial |
| Greek | el | 1,330 | 28 | partial; native review and these remaining keys are outstanding |
| Hungarian | hu | 1,326 | 28 | partial; native proofreading remains pending |
| Italian | it | 1,253 | 99 | partial; native proofreading remains pending |
| Korean | ko | 1,235 | 123 | partial; Hangul font and platform IME constraints apply |
| Russian | ru | 1,005 | 345 | partial |
| Spanish | es | 1,243 | 114 | partial |
| Swedish | sv | 1,330 | 28 | partial; native terminology review is recorded |
| Turkish | tr | 1,330 | 28 | partial; native review is recorded |
| Ukrainian | uk | 1,350 | 0 | reference catalog; required coverage tests pass |
| Simplified Chinese | zh | 1,142 | 230 | partial; CJK font and IME constraints apply |
| Hindi | hi | 0 | — | no catalog; Devanagari shaping is not implemented |
| Indonesian | id | 0 | — | no catalog |
| Japanese | ja | 0 | — | no catalog; CJK font and IME work remains |
| Vietnamese | vi | 0 | — | no catalog; stacked-diacritic layout is unverified |

Counts describe catalog data, not translation quality. Native-speaker review is still needed where
not explicitly recorded above. Engine diagnostics and arbitrary command output remain English.
The supported integer plural forms are implemented per language; CLDR defines the categories used
by languages with one, few, many, and other forms, among others ([Unicode CLDR plural rules](https://cldr.unicode.org/index/cldr-spec/plural-rules)).

CADCraft currently embeds 15 non-English catalogs. This is broader than the 7 non-English
localizations measured in the installed AutoCAD for Mac bundle; the comparison does not imply equal
translation completeness, font support, IME reliability, or native review. The catalog tests and
this table measure different things, so use the measured missing-key counts above rather than the
older estimated completion percentages.

**Remaining work:** finish the reference-key gaps in existing catalogs; add catalogs for Hindi,
Indonesian, Japanese and Vietnamese; validate Arabic UI mirroring and platform IME behavior; and
obtain native-speaker review for all AI-assisted or unreviewed translations. Script shaping and
IME correctness are shared engineering work, not translation-only tasks.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-23 | major | Recounted all 15 checked-in catalogs and compared key coverage with the 1,350-key Ukrainian reference; corrected stale language counts and completion labels |
| 2026-10-15 | minor | Greek (`el`) catalog: 1,330 rows, context/plural/locale/font tests; review pending |
| 2026-10-14 | minor | Swedish completed: 95 rows for strings added since (Hatch and Gradient dialog, Block Editor, REFEDIT, multileader editing, Options selection settings, MTEXT editor, recent files); 1,330 rows, every key of the Ukrainian reference catalog |
| 2026-10-18 | minor | Turkish added: 1,330 rows (the existing catalog union), single count form, four menu headings, prompt keywords kept in English; locale/preferences/menu/live-switch/dialog/font tests; no gaps in the marked UI literals; AI-assisted, the running app reviewed by a native speaker |
| 2026-10-13 | minor | Hungarian added: 1,326 rows, a single count form, menu headings, locale/preferences/menu/font/render tests; no gaps in the marked UI literals; native proofreading pending |
| 2026-10-13 | minor | Swedish added: 1,235 rows, one/other plurals, menu headings, locale/preferences/menu/font/render tests; no gaps in the marked UI literals; terminology and the running app reviewed by a native speaker |
| 2026-10-13 | minor | Korean added: full 1,235-row existing catalog union, single count form, locale/persistence/live-switch tests, and validated Hangul UI fonts; native review and platform verification pending |
| 2026-10-12 | minor | Italian added: 1,227 rows, one/other plurals, two `menu` headings, locale/preference/plural/font tests; AI-assisted, native proofreading pending |
| 2026-10-12 | minor | Brazilian Portuguese catalog completed by @RicSchonfelder: 238 rows added (1,231 total), now covering the reference catalog and every marked UI literal, gaps 0; bundled-font glyph test for the catalog's accented letters |
| 2026-10-11 | minor | Czech added: 1,202 rows, one/few/other plurals, locale/preferences/menu/font/render tests; independent review pass; native proofreading pending |
| 2026-10-11 | minor | German catalog landed: `de` preference and catalog, per-language plural rules; non-English languages 1 → 2; total hours 60–100 → 55–95 |
| 2026-10-10 | minor | Right-to-left drawing text: bidi and Arabic-script shaping in TEXT and MTEXT (#287) |
| 2026-10-10 | minor | Ukrainian catalog landed on main (#36): catalog infrastructure exists, Ukrainian partial; total hours 70–110 → 60–100 |
| 2026-10-10 | major | First localization measurement against AutoCAD 2027 for Mac's bundle localizations |
