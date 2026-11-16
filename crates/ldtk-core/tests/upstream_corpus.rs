use ldtk_core::Project;
use serde_json::Value;
use std::{fs, path::Path};

fn collect(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "ldtk") {
            files.push(path);
        }
    }
}

#[test]
#[ignore = "requires pinned upstream checkout under .reference/ldtk"]
fn every_upstream_project_round_trips_with_all_json_values_preserved() {
    let root = std::env::var_os("LDTK_REFERENCE_TESTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.reference/ldtk/tests")
        });
    let mut paths = vec![];
    collect(&root, &mut paths);
    assert!(paths.len() >= 49);
    let mut external_count = 0;
    for path in &paths {
        let temp = tempfile::tempdir().unwrap();
        let mut project = Project::load(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let expected = project.json.clone();
        let external = project.external_levels.clone();
        external_count += external.len();
        // This test checks serialization fidelity only. Assets intentionally stay at source.
        project.path = None;
        for rel in external.keys() {
            fs::create_dir_all(temp.path().join(rel).parent().unwrap()).unwrap();
        }
        let output = temp.path().join("round-trip.ldtk");
        project
            .save(&output)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let reloaded = Project::load(&output).unwrap();
        assert_eq!(reloaded.json, expected, "{}", path.display());
        assert_eq!(reloaded.external_levels, external, "{}", path.display());
        let raw: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
        assert_eq!(raw, expected, "{}", path.display());
    }
    eprintln!(
        "Verified {} original project documents and {external_count} external levels",
        paths.len()
    );
}
