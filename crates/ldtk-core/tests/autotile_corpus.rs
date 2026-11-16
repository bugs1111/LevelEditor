//! Historical fixtures contain deliberately broken/outdated caches. This audit
//! verifies that every source rebuilds deterministically, while reporting (not
//! suppressing) saved-cache differences. Current official samples are checked
//! for exact saved-cache parity in autotile.rs.
use ldtk_core::Project;
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

fn collect(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|e| e == "ldtk") {
            files.push(path);
        }
    }
}

#[test]
#[ignore = "requires pinned upstream historical corpus; run in release mode"]
fn historical_corpus_rebuild_is_deterministic_and_reports_cache_differences() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.reference/ldtk/tests");
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    assert!(paths.len() >= 49);
    let mut layers = 0;
    let mut matching = 0;
    let mut differences = BTreeMap::<String, usize>::new();
    for path in &paths {
        let mut project = Project::load(path).unwrap();
        for key in project.level_keys() {
            let mut old = project.level(key).unwrap().clone();
            // LDtk versions before 1.4 omitted a=1 from tile records.
            for layer in old["layerInstances"].as_array_mut().unwrap() {
                for tile in layer["autoLayerTiles"].as_array_mut().unwrap() {
                    if tile.get("a").is_none() {
                        tile["a"] = json!(1);
                    }
                }
            }
            for layer in project.level_mut(key).unwrap()["layerInstances"]
                .as_array_mut()
                .unwrap()
            {
                layer["autoLayerTiles"] = json!([]);
            }
            project
                .rebuild_auto_layers(key)
                .unwrap_or_else(|e| panic!("{} {key:?}: {e}", path.display()));
            let rebuilt = project.level(key).unwrap().clone();
            project.rebuild_auto_layers(key).unwrap();
            assert_eq!(
                project.level(key).unwrap(),
                &rebuilt,
                "{} {key:?}",
                path.display()
            );
            for (index, layer) in old["layerInstances"].as_array().unwrap().iter().enumerate() {
                layers += 1;
                if rebuilt["layerInstances"][index]["autoLayerTiles"] == layer["autoLayerTiles"] {
                    matching += 1;
                } else {
                    *differences
                        .entry(path.strip_prefix(&root).unwrap().display().to_string())
                        .or_default() += 1;
                }
            }
        }
    }
    eprintln!(
        "Rebuilt {} projects / {layers} layers deterministically; {matching} saved caches match after legacy alpha normalization; {} differ",
        paths.len(),
        layers - matching
    );
    for (file, count) in differences {
        eprintln!("{file}: {count} differing saved layers");
    }
}

#[test]
#[ignore = "original 13MB load and rebuild timing"]
fn original_large_level_load_and_rebuild_timing() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/tests/largeLevels/world1.ldtk");
    let started = std::time::Instant::now();
    let mut project = Project::load(path).unwrap();
    eprintln!("13MB load {:?}", started.elapsed());
    let started = std::time::Instant::now();
    let migrated = project.migrate_to_current().unwrap();
    eprintln!("13MB migrate={migrated} {:?}", started.elapsed());
    for key in project.level_keys() {
        let started = std::time::Instant::now();
        project.rebuild_auto_layers(key).unwrap();
        eprintln!("13MB level {key:?} rebuild {:?}", started.elapsed());
    }
}
