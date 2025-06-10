use crate::bulk::types::BulkProcessedItem;
use log::warn;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Renders a simple template string by replacing {key} with values from the map.
///
/// # Arguments
/// * `template_str`: The template string, e.g., "{manifest_label}_{page_number}".
/// * `vars`: A map of variable names to their string values.
///
/// # Returns
/// The rendered string or an error if a key is not found (currently returns template with missing keys).
fn render_template(template_str: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template_str.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

/// Helper to check if any variable in the template string exists in the provided vars map
fn vars_can_render_template(template_str: &str, vars: &HashMap<String, String>) -> bool {
    let mut i = 0;
    while let Some(start) = template_str[i..].find('{') {
        if let Some(end) = template_str[i + start..].find('}') {
            let key = &template_str[i + start + 1..i + start + end];
            if vars.contains_key(key) {
                return true;
            }
            i += start + end + 1;
        } else {
            break;
        }
    }
    false
}

/// Generates the output file path for a single bulk item.
///
/// The generated path does not include an extension; it's expected that
/// the dezooming process will add an appropriate extension based on image content.
///
/// # Arguments
/// * `output_directory`: The base directory for output files.
/// * `output_template`: Optional user-defined filename template (relative to `output_directory`).
/// * `item`: The `BulkProcessedItem` containing `template_vars` and `default_filename_stem`.
/// * `item_index_0_based`: The 0-based index of the current item in the bulk list.
/// * `total_items`: Total number of items in the bulk list, for index padding.
///
/// # Returns
/// A `PathBuf` for the output file (stem only, no extension).
pub fn generate_output_path_for_item(
    output_directory: &Path,
    output_template: Option<&str>,
    item: &BulkProcessedItem,
    item_index_0_based: usize,
    total_items: usize,
) -> PathBuf {
    let filename_index_1_based = item_index_0_based + 1;
    let num_digits_in_total = if total_items == 0 {
        1
    } else {
        (total_items as f64).log10().floor() as usize + 1
    };
    let padding_width = num_digits_in_total.max(4);

    let padded_index = format!("{:0width$}", filename_index_1_based, width = padding_width);

    let filename_stem_str: String = match output_template {
        Some(template_str) => {
            let mut effective_vars = item.template_vars.clone();
            effective_vars.insert("index".to_string(), padded_index.clone());
            effective_vars.insert("item_index".to_string(), item_index_0_based.to_string());
            effective_vars.insert("item_index_1".to_string(), padded_index.clone());
            effective_vars.insert(
                "page_number".to_string(),
                filename_index_1_based.to_string(),
            );
            effective_vars.insert("total_items".to_string(), total_items.to_string());
            effective_vars.insert(
                "default_stem".to_string(),
                item.default_filename_stem.clone(),
            );

            let rendered = render_template(template_str, &effective_vars);
            if rendered.is_empty()
                || (rendered == template_str
                    && template_str.contains('{')
                    && !vars_can_render_template(template_str, &effective_vars))
            {
                warn!(
                    "Template rendering for '{}' resulted in an empty or effectively unchanged string using available variables. Falling back to default naming with index: {} and default stem: {}",
                    template_str, padded_index, item.default_filename_stem
                );
                format!("{}_{}", item.default_filename_stem, padded_index)
            } else {
                rendered
            }
        }
        None => {
            if item.default_filename_stem.trim().is_empty() {
                format!("item_{}", padded_index)
            } else {
                format!("{}_{}", item.default_filename_stem, padded_index)
            }
        }
    };

    let final_filename_stem_str = if filename_stem_str.trim().is_empty() {
        format!("item_{}", padded_index)
    } else {
        filename_stem_str
    };

    output_directory.join(final_filename_stem_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_template_simple() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "world".to_string());
        vars.insert("num".to_string(), "123".to_string());
        let template = "Hello, {name}! Count: {num}.";
        assert_eq!(
            render_template(template, &vars),
            "Hello, world! Count: 123."
        );
    }

    #[test]
    fn test_render_template_missing_key() {
        let vars = HashMap::new();
        let template = "Key: {missing_key}";
        assert_eq!(render_template(template, &vars), "Key: {missing_key}");
    }

    #[test]
    fn test_generate_output_path_no_template() {
        let dir = PathBuf::from("output");
        let item = BulkProcessedItem {
            download_url: "http://example.com/image.jpg".to_string(),
            template_vars: HashMap::new(),
            default_filename_stem: "default_stem".to_string(),
        };
        let path = generate_output_path_for_item(&dir, None, &item, 0, 10);
        assert_eq!(path, dir.join("default_stem_0001"));

        let path_high_index = generate_output_path_for_item(&dir, None, &item, 9, 10);
        assert_eq!(path_high_index, dir.join("default_stem_0010"));

        let path_high_total = generate_output_path_for_item(&dir, None, &item, 0, 10000);
        assert_eq!(path_high_total, dir.join("default_stem_00001"));
    }

    #[test]
    fn test_generate_output_path_with_template() {
        let dir = PathBuf::from("custom_output");
        let mut vars = HashMap::new();
        vars.insert("id".to_string(), "item123".to_string());
        vars.insert("label".to_string(), "My Label".to_string());

        let item = BulkProcessedItem {
            download_url: "url".to_string(),
            template_vars: vars,
            default_filename_stem: "fallback".to_string(),
        };

        let template1 = "{label}_{id}_{index}";
        let path1 = generate_output_path_for_item(&dir, Some(template1), &item, 0, 1);
        assert_eq!(path1, dir.join("My Label_item123_0001"));

        let template2 = "{default_stem}_extra_{item_index_1}";
        let path2 = generate_output_path_for_item(&dir, Some(template2), &item, 2, 5);
        assert_eq!(path2, dir.join("fallback_extra_0003"));

        let template3 = "subdir/{id}/{index}";
        let path3 = generate_output_path_for_item(&dir, Some(template3), &item, 0, 1);
        assert_eq!(path3, dir.join("subdir/item123/0001"));
    }

    #[test]
    fn test_generate_output_path_empty_template_render_fallback() {
        let dir = PathBuf::from("output");
        let item = BulkProcessedItem {
            download_url: "url".to_string(),
            template_vars: HashMap::new(),
            default_filename_stem: "default_fallback".to_string(),
        };
        let template = "{unknown_var}";
        let path = generate_output_path_for_item(&dir, Some(template), &item, 0, 1);
        assert_eq!(path, dir.join("default_fallback_0001"));
    }

    #[test]
    fn test_generate_output_path_empty_default_stem_no_template() {
        let dir = PathBuf::from("output");
        let item = BulkProcessedItem {
            download_url: "url".to_string(),
            template_vars: HashMap::new(),
            default_filename_stem: "".to_string(),
        };
        let path = generate_output_path_for_item(&dir, None, &item, 0, 1);
        assert_eq!(path, dir.join("item_0001"));
    }
}
