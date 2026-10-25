//! Library ids changed where a design refers to them: a project part renamed, or parts pasted
//! from one studio's project library into another's. Symbols are named by their id in the
//! schematic's definitions and by the symbols placed from them; footprints by the placed
//! symbols' Footprint fields, the definitions' default ones, and the footprints on the board.

use crate::Design;
use crate::schematic::PlacedSymbol;
use crate::symbol::{Symbol, fields};

/// A map of library ids: the new id for an old one, `None` to keep it.
pub type IdMap<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Renames the symbol ids `sym` maps and the footprint ids `fp` maps throughout `d`.
pub fn rename_parts(d: &mut Design, sym: IdMap, fp: IdMap) {
    rename_in_schematic(&mut d.schematic.symbols, d.schematic.sheets.iter_mut().flat_map(|s| s.symbols.iter_mut()), sym, fp);
    for f in &mut d.board.footprints {
        if let Some(n) = fp(&f.footprint.id) {
            f.footprint.id = n;
        }
    }
}

/// Renames the ids in symbol definitions and in symbols placed from them (a schematic's, or a
/// clipboard's).
pub fn rename_in_schematic<'a>(defs: &mut [Symbol], placed: impl Iterator<Item = &'a mut PlacedSymbol>, sym: IdMap, fp: IdMap) {
    let footprint = |f: &mut crate::symbol::Field| {
        if f.name == fields::FOOTPRINT
            && let Some(n) = fp(&f.text.text)
        {
            f.text.text = n;
        }
    };
    for d in defs {
        if let Some(n) = sym(&d.id) {
            d.id = n;
        }
        d.fields.iter_mut().for_each(footprint);
    }
    for s in placed {
        if let Some(n) = sym(&s.symbol) {
            s.symbol = n;
        }
        s.fields.iter_mut().for_each(footprint);
    }
}

/// `lib:name` with the library `from` swapped for `to`; `None` for an id in another library.
pub fn reprefix(id: &str, from: &str, to: &str) -> Option<String> {
    let (lib, name) = id.split_once(':')?;
    (lib == from && from != to).then(|| format!("{to}:{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_follow_a_rename() {
        let mut d = crate::getting_started::gs16(&crate::library::LibraryTable::builtin());
        let r = d.schematic.sheets[0].symbols.iter().find(|s| s.reference() == "R1").unwrap();
        let (old_sym, old_fp) = (r.symbol.clone(), r.field(fields::FOOTPRINT).unwrap().value().to_string());
        rename_parts(&mut d, &|id| (id == old_sym).then(|| "Proj:R_new".into()), &|id| (id == old_fp).then(|| "Proj:FP_new".into()));
        let r = d.schematic.sheets[0].symbols.iter().find(|s| s.reference() == "R1").unwrap();
        assert_eq!(r.symbol, "Proj:R_new");
        assert_eq!(r.field(fields::FOOTPRINT).unwrap().value(), "Proj:FP_new");
        assert!(d.schematic.symbol("Proj:R_new").is_some());
        assert!(d.board.footprints.iter().any(|f| f.footprint.id == "Proj:FP_new"));
        assert_eq!(reprefix("A:x", "A", "B").as_deref(), Some("B:x"));
        assert_eq!(reprefix("C:x", "A", "B"), None);
    }
}
