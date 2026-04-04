use std::collections::HashMap;

/// Maps function names to their expected file extensions.
/// If the first arg is a filename with wrong extension → AG006.
pub fn function_extension_map() -> HashMap<&'static str, (&'static [&'static str], &'static str)> {
    let mut m: HashMap<&str, (&[&str], &str)> = HashMap::new();

    // pandas read functions
    m.insert(
        "read_csv",
        (
            &[".csv", ".tsv", ".txt", ".gz"],
            "read_csv expects .csv files",
        ),
    );
    m.insert(
        "read_excel",
        (
            &[".xlsx", ".xls", ".xlsm", ".xlsb", ".ods"],
            "read_excel expects .xlsx/.xls files",
        ),
    );
    m.insert(
        "read_json",
        (
            &[".json", ".jsonl", ".ndjson", ".gz"],
            "read_json expects .json files",
        ),
    );
    m.insert(
        "read_parquet",
        (&[".parquet", ".pq"], "read_parquet expects .parquet files"),
    );
    m.insert(
        "read_feather",
        (&[".feather", ".ftr"], "read_feather expects .feather files"),
    );
    m.insert(
        "read_hdf",
        (
            &[".h5", ".hdf5", ".hdf", ".he5"],
            "read_hdf expects .h5/.hdf5 files",
        ),
    );
    m.insert("read_stata", (&[".dta"], "read_stata expects .dta files"));
    m.insert(
        "read_sas",
        (&[".sas7bdat", ".xpt"], "read_sas expects .sas7bdat files"),
    );
    m.insert(
        "read_spss",
        (&[".sav", ".zsav"], "read_spss expects .sav files"),
    );
    m.insert(
        "read_pickle",
        (&[".pkl", ".pickle", ".p"], "read_pickle expects .pkl files"),
    );
    m.insert("read_xml", (&[".xml"], "read_xml expects .xml files"));
    m.insert(
        "read_html",
        (&[".html", ".htm"], "read_html expects .html files"),
    );

    // to_ functions
    m.insert(
        "to_csv",
        (
            &[".csv", ".tsv", ".txt", ".gz"],
            "to_csv expects .csv output",
        ),
    );
    m.insert(
        "to_excel",
        (&[".xlsx", ".xls"], "to_excel expects .xlsx output"),
    );
    m.insert(
        "to_json",
        (&[".json", ".jsonl"], "to_json expects .json output"),
    );
    m.insert(
        "to_parquet",
        (&[".parquet", ".pq"], "to_parquet expects .parquet output"),
    );

    // json module
    m.insert(
        "load",
        (&[".json", ".jsonl"], "json.load expects .json files"),
    );

    // PIL/cv2
    m.insert(
        "imread",
        (
            &[".png", ".jpg", ".jpeg", ".bmp", ".tiff", ".gif", ".webp"],
            "imread expects image files",
        ),
    );
    m.insert(
        "imwrite",
        (
            &[".png", ".jpg", ".jpeg", ".bmp", ".tiff", ".webp"],
            "imwrite expects image output",
        ),
    );

    m
}

/// Check if a filename matches expected extensions for a function.
/// Returns Some(mismatch_message) if it doesn't match, None if it does.
pub fn check_extension_match(function: &str, filename: &str) -> Option<String> {
    let map = function_extension_map();
    let (expected_exts, desc) = map.get(function)?;

    // Extract extension from filename
    let lower = filename.to_lowercase();
    let has_matching_ext = expected_exts.iter().any(|ext| lower.ends_with(ext));

    if has_matching_ext {
        return None;
    }

    // Only flag if filename actually has a recognizable extension
    let has_any_ext = lower
        .rfind('.')
        .is_some_and(|pos| pos > 0 && pos < lower.len() - 1);
    if !has_any_ext {
        return None;
    }

    let actual_ext = &lower[lower.rfind('.').unwrap()..];
    let expected_str = expected_exts.join(", ");
    Some(format!(
        "{desc}, but got '{actual_ext}' (expected: {expected_str})"
    ))
}
