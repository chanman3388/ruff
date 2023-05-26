// use log::debug;
use std::path::Path;

use rustc_hash::{FxHashMap, FxHashSet};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::to_module_path;
use ruff_python_ast::imports::ModuleImport;

#[violation]
pub struct CyclicImport {
    pub cycle: String,
}

impl Violation for CyclicImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cyclic import ({}) (cyclic-import)", self.cycle)
    }
}

struct VisitedAndCycles<'a> {
    fully_visited: FxHashSet<&'a str>,
    cycles: Option<FxHashSet<Vec<&'a str>>>,
}

impl<'a> VisitedAndCycles<'a> {
    fn new(fully_visited: FxHashSet<&'a str>, cycles: FxHashSet<Vec<&'a str>>) -> Self {
        if cycles.is_empty() {
            Self {
                fully_visited,
                cycles: None,
            }
        } else {
            Self {
                fully_visited,
                cycles: Some(cycles),
            }
        }
    }
}

struct CyclicImportChecker<'a> {
    imports: &'a FxHashMap<String, Vec<ModuleImport>>,
}

impl<'a> CyclicImportChecker<'a> {
    fn has_cycles(&self, name: &'a str) -> VisitedAndCycles<'a> {
        // we check before hand that the name is in the imports, ergo it will be in the module mapping and thus this unwrap is safe
        let mut stack: Vec<&str> = vec![name];
        let mut fully_visited: FxHashSet<&str> = FxHashSet::default();
        let mut cycles: FxHashSet<Vec<&str>> = FxHashSet::default();
        self.has_cycles_helper(
            name,
            &mut stack,
            &mut cycles,
            &mut fully_visited,
            // 0,
        );
        VisitedAndCycles::new(fully_visited, cycles)
    }

    fn has_cycles_helper(
        &self,
        name: &'a str,
        stack: &mut Vec<&'a str>,
        cycles: &mut FxHashSet<Vec<&'a str>>,
        fully_visited: &mut FxHashSet<&'a str>,
        // level: usize,
    ) {
        if let Some(imports) = self.imports.get(name) {
            // let tabs = "\t".repeat(level);
            // debug!("{tabs}{name}");
            for import in imports.iter() {
                // debug!("{tabs}\timport: {}", import.module);
                if let Some(idx) = stack.iter().position(|s| s == &import.module) {
                    // debug!("{tabs}\t\t cycles: {:?}", stack[idx..].to_vec());
                    // when the length is 1 and the only item is the import we're looking at
                    // then we're importing self, could we report this so we don't have to
                    // do this again for import-self W0406?
                    if stack[idx..].len() == 1 && stack[idx] == name {
                        continue;
                    }
                    cycles.insert(stack[idx..].to_vec());
                } else {
                    stack.push(&import.module);
                    self.has_cycles_helper(
                        &import.module,
                        stack,
                        cycles,
                        fully_visited,
                        // level + 1,
                    );
                    stack.pop();
                }
            }
        }
        fully_visited.insert(name);
    }
}

/// PLR0914
pub fn cyclic_import<'a>(
    path: &Path,
    package: Option<&Path>,
    imports: &'a FxHashMap<String, Vec<ModuleImport>>,
    cycles: &mut FxHashMap<&'a str, FxHashSet<Vec<&'a str>>>,
) -> Option<Vec<Diagnostic>> {
    let Some(package) = package else {
        return None;
    };
    let Some(module_name) = to_module_path(package, path) else {
        return None;
    };
    let module_name = module_name.join(".");
    // if the module name isn't in the import map, it can't possibly have cycles
    // this also allows us to use `unwrap` whenever we use methods on the `ModuleMapping`
    // as any modules as part of cycles are guaranteed to be in the `ModuleMapping`
    // debug!("Checking module {module_name}");
    let Some((module_name, _)) = imports.get_key_value(&module_name as &str) else {
        return None;
    };
    if let Some(existing_cycles) = cycles.get(module_name as &str) {
        if existing_cycles.is_empty() {
            return None;
        }
        // debug!("Existing cycles: {existing_cycles:#?}");
        Some(
            existing_cycles
                .iter()
                .map(|cycle| {
                    Diagnostic::new(
                        CyclicImport {
                            cycle: cycle.join(" -> "),
                        },
                        (&imports[module_name][0]).into(),
                    )
                })
                .collect::<Vec<Diagnostic>>(),
        )
    } else {
        let cyclic_import_checker = CyclicImportChecker { imports };
        let VisitedAndCycles {
            fully_visited: mut visited,
            cycles: new_cycles,
        } = cyclic_import_checker.has_cycles(module_name);
        // we'll always have new visited stuff if we have
        let mut out_vec: Vec<Diagnostic> = Vec::new();
        if let Some(new_cycles) = new_cycles {
            // debug!("New cycles {new_cycles:#?}");
            for new_cycle in &new_cycles {
                if let [first, the_rest @ ..] = &new_cycle[..] {
                    if first == module_name {
                        out_vec.push(Diagnostic::new(
                            CyclicImport {
                                cycle: new_cycle.join(" -> "),
                            },
                            imports[module_name]
                                .iter()
                                .find(|m| m.module == the_rest[0])
                                .unwrap()
                                .into(),
                        ));
                    }
                }
                for involved_module in new_cycle.iter() {
                    // we re-order the cycles for the modules involved here
                    let pos = new_cycle.iter().position(|s| s == involved_module).unwrap();
                    let cycle_to_insert = new_cycle[pos..]
                        .iter()
                        .chain(new_cycle[..pos].iter())
                        .map(std::clone::Clone::clone)
                        .collect::<Vec<_>>();
                    if let Some(existing) = cycles.get_mut(involved_module) {
                        existing.insert(cycle_to_insert);
                    } else {
                        let mut new_set = FxHashSet::default();
                        new_set.insert(cycle_to_insert);
                        cycles.insert(*involved_module, new_set);
                    }
                    visited.remove(involved_module);
                }
            }
        }
        // process the visited nodes which don't have cycles
        for visited_module in visited {
            cycles.insert(visited_module, FxHashSet::default());
        }
        if out_vec.is_empty() {
            None
        } else {
            Some(out_vec)
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::imports::ImportMap;
    use ruff_text_size::{TextRange, TextSize};

    use super::*;

    #[test]
    fn cyclic_import_unrelated_module_not_traversed() {
        let mut map = FxHashMap::default();
        let size1 = TextSize::from(1);
        let size2 = TextSize::from(2);
        let range1 = TextRange::new(size1, size2);
        let range2 = TextRange::new(size1, size2);

        let a = ModuleImport::new("a".to_string(), range1);
        let b = ModuleImport::new("b".to_string(), range2);
        map.insert(a.module.clone(), vec![]);
        map.insert(b.module, vec![a.clone()]);
        let import_map = ImportMap::new(map);
        let cyclic_checker = CyclicImportChecker {
            imports: &import_map.module_to_imports,
        };

        let VisitedAndCycles {
            fully_visited: visited,
            cycles,
        } = cyclic_checker.has_cycles(&a.module);
        let mut check_visited: FxHashSet<&str> = FxHashSet::default();
        check_visited.insert(&a.module);
        assert_eq!(visited, check_visited);
        assert!(cycles.is_none());
    }

    #[test]
    fn cyclic_import_multiple_cycles() {
        let mut map = FxHashMap::default();
        let size1 = TextSize::from(1);
        let size2 = TextSize::from(2);
        let size3 = TextSize::from(3);
        let size4 = TextSize::from(4);
        let range1 = TextRange::new(size1, size2);
        let range2 = TextRange::new(size1, size3);
        let range3 = TextRange::new(size1, size4);
        let range4 = TextRange::new(size2, size3);

        let a = ModuleImport::new("a".to_string(), range1);
        let b = ModuleImport::new("b".to_string(), range2);
        let c = ModuleImport::new("c".to_string(), range3);
        let d = ModuleImport::new("d".to_string(), range4);

        map.insert(a.module.clone(), vec![b.clone(), c.clone()]);
        map.insert(b.module.clone(), vec![c.clone(), d.clone()]);
        map.insert(c.module.clone(), vec![b.clone(), d.clone()]);
        map.insert(d.module.clone(), vec![a.clone()]);
        let import_map = ImportMap::new(map);
        let cyclic_checker = CyclicImportChecker {
            imports: &import_map.module_to_imports,
        };

        let VisitedAndCycles {
            fully_visited: visited,
            cycles,
        } = cyclic_checker.has_cycles(&a.module);

        let mut check_visited: FxHashSet<&str> = FxHashSet::default();
        check_visited.insert(&a.module);
        check_visited.insert(&b.module);
        check_visited.insert(&c.module);
        check_visited.insert(&d.module);
        assert_eq!(visited, check_visited);

        let mut check_cycles: FxHashSet<Vec<&str>> = FxHashSet::default();
        check_cycles.insert(vec![&a.module, &b.module, &c.module, &d.module]);
        check_cycles.insert(vec![&a.module, &c.module, &b.module, &d.module]);
        check_cycles.insert(vec![&a.module, &c.module, &d.module]);
        check_cycles.insert(vec![&a.module, &b.module, &d.module]);
        check_cycles.insert(vec![&b.module, &c.module]);
        check_cycles.insert(vec![&c.module, &b.module]);
        assert_eq!(cycles, Some(check_cycles));
    }

    #[test]
    fn cyclic_import_check_diagnostics() {
        let size1 = TextSize::from(1);
        let size2 = TextSize::from(2);
        let size3 = TextSize::from(3);
        let size4 = TextSize::from(4);
        let range1 = TextRange::new(size1, size2);
        let range2 = TextRange::new(size1, size3);
        let range3 = TextRange::new(size1, size4);
        let range4 = TextRange::new(size2, size3);
        let range5 = TextRange::new(size2, size4);

        let a_a = ModuleImport::new("a.a".to_string(), range1);
        let a_b = ModuleImport::new("a.b".to_string(), range2);
        let a_c = ModuleImport::new("a.c".to_string(), range3);
        let b_in_a = ModuleImport::new("a.b".to_string(), range4);
        let a_in_b = ModuleImport::new("a.a".to_string(), range5);
        let mut map = FxHashMap::default();
        map.insert(a_a.module.clone(), vec![b_in_a.clone()]);
        map.insert(a_b.module.clone(), vec![a_in_b.clone()]);
        map.insert(a_c.module, vec![]);
        let import_map = ImportMap::new(map);

        let path_a = Path::new("a/a");
        let path_b = Path::new("a/b");
        let path_c = Path::new("a/c");
        let package = Some(Path::new("a"));

        let mut cycles = FxHashMap::default();
        let diagnostic = cyclic_import(path_a, package, &import_map.module_to_imports, &mut cycles);

        let mut set_a: FxHashSet<Vec<&str>> = FxHashSet::default();
        set_a.insert(vec![&a_b.module, &a_a.module]);
        let mut set_b: FxHashSet<Vec<&str>> = FxHashSet::default();
        set_b.insert(vec![&a_a.module, &a_b.module]);

        assert_eq!(
            diagnostic,
            Some(vec![Diagnostic::new(
                CyclicImport {
                    cycle: "a.a -> a.b".to_string(),
                },
                (&b_in_a).into(),
            )])
        );
        let mut check_cycles: FxHashMap<&str, FxHashSet<Vec<&str>>> = FxHashMap::default();
        check_cycles.insert(&a_b.module, set_a);
        check_cycles.insert(&a_a.module, set_b);
        assert_eq!(cycles, check_cycles);

        let diagnostic = cyclic_import(path_b, package, &import_map.module_to_imports, &mut cycles);
        assert_eq!(
            diagnostic,
            Some(vec![Diagnostic::new(
                CyclicImport {
                    cycle: "a.b -> a.a".to_string(),
                },
                (&a_in_b).into(),
            )])
        );
        assert!(
            cyclic_import(path_c, package, &import_map.module_to_imports, &mut cycles).is_none()
        );
    }

    #[test]
    fn cyclic_import_test_no_cycles_on_import_self() {
        let size1 = TextSize::from(1);
        let size2 = TextSize::from(2);
        let range = TextRange::new(size1, size2);
        let a = ModuleImport::new("a".to_string(), range);
        let mut map = FxHashMap::default();
        map.insert(a.module.clone(), vec![a.clone()]);

        let import_map = ImportMap::new(map);

        let cyclic_checker = CyclicImportChecker {
            imports: &import_map.module_to_imports,
        };
        let VisitedAndCycles {
            fully_visited: visited,
            cycles,
        } = cyclic_checker.has_cycles(&a.module);

        let mut check_visited: FxHashSet<&str> = FxHashSet::default();
        check_visited.insert(&a.module);
        assert_eq!(visited, check_visited);

        assert!(cycles.is_none());
    }
}
