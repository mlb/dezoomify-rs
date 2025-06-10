use crate::bulk::types::{BulkInputParser, BulkProcessedItem};
use crate::iiif::manifest_types::{ExtractedImageInfo, Manifest};
use serde_json;
use std::collections::HashMap;

fn sanitize_for_filename(name: &str) -> String {
    name.replace(' ', "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// A parser for IIIF Manifests.
/// It extracts image information from a manifest and converts it into `BulkProcessedItem`s.
#[derive(Default, Debug)]
pub struct IiifManifestBulkParser;

impl IiifManifestBulkParser {
    pub fn new() -> Self {
        IiifManifestBulkParser
    }
}

impl BulkInputParser for IiifManifestBulkParser {
    fn name(&self) -> &str {
        "IiifManifestBulkParser"
    }

    async fn parse(
        &self,
        content: &str,
        source_url: Option<&str>,
    ) -> Result<Vec<BulkProcessedItem>, String> {
        let manifest: Manifest = serde_json::from_str(content)
            .map_err(|e| format!("Failed to parse IIIF Manifest JSON: {}", e))?;

        let extracted_infos = manifest.extract_image_infos(source_url.unwrap_or(""));

        if extracted_infos.is_empty() {
            return Ok(Vec::new());
        }

        let total_pages = extracted_infos.len();
        let mut bulk_items = Vec::new();

        for info in extracted_infos.into_iter() {
            let ExtractedImageInfo {
                image_uri,
                manifest_label,
                canvas_label,
                canvas_index,
            } = info;

            let page_number = canvas_index + 1;

            let manifest_label_str = manifest_label.unwrap_or_else(|| match &manifest.label {
                crate::iiif::manifest_types::IiifLabel::String(s) if s.is_empty() => "".to_string(),
                _ => "None".to_string(),
            });
            let canvas_label_str = canvas_label.unwrap_or_else(|| {
                if let Some(canvas) = manifest.items.get(canvas_index) {
                    match &canvas.label {
                        crate::iiif::manifest_types::IiifLabel::String(s) if s.is_empty() => {
                            "".to_string()
                        }
                        _ => "None".to_string(),
                    }
                } else {
                    "None".to_string()
                }
            });

            let mut template_vars = HashMap::new();
            template_vars.insert("manifest_label".to_string(), manifest_label_str.clone());
            template_vars.insert("canvas_label".to_string(), canvas_label_str.clone());
            template_vars.insert("page_number".to_string(), page_number.to_string());
            template_vars.insert("total_pages".to_string(), total_pages.to_string());
            template_vars.insert("canvas_index".to_string(), canvas_index.to_string());
            template_vars.insert("image_uri".to_string(), image_uri.clone());

            let sanitized_m_label = sanitize_for_filename(&manifest_label_str);

            let default_filename_stem =
                if !sanitized_m_label.is_empty() && sanitized_m_label != "None" {
                    format!("{}_page_{}", sanitized_m_label, page_number)
                } else {
                    format!("manifest_page_{}", page_number)
                };

            let final_default_filename_stem = if default_filename_stem.is_empty() {
                format!("item_{}", page_number)
            } else {
                default_filename_stem
            };

            bulk_items.push(BulkProcessedItem {
                download_url: image_uri,
                template_vars,
                default_filename_stem: final_default_filename_stem,
            });
        }

        Ok(bulk_items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_minimal_manifest_json(
        manifest_id: &str,
        manifest_label_val: serde_json::Value,
        canvas_id_prefix: &str,
        canvas_label_val: serde_json::Value,
        image_service_id: &str,
        num_canvases: usize,
    ) -> String {
        let mut items = Vec::new();
        for i in 0..num_canvases {
            items.push(json!({
                "id": format!("{}/canvas/{}", canvas_id_prefix, i),
                "type": "Canvas",
                "label": canvas_label_val.clone(),
                "height": 1000,
                "width": 800,
                "items": [
                    {
                        "id": format!("{}/page/{}/annotationpage", canvas_id_prefix, i),
                        "type": "AnnotationPage",
                        "items": [
                            {
                                "id": format!("{}/page/{}/annotation/1", canvas_id_prefix, i),
                                "type": "Annotation",
                                "motivation": "painting",
                                "body": {
                                    "id": format!("{}/full/full/0/default.jpg", image_service_id),
                                    "type": "Image",
                                    "format": "image/jpeg",
                                    "service": [
                                        {
                                            "@id": image_service_id,
                                            "type": "ImageService3",
                                            "profile": "level2"
                                        }
                                    ],
                                    "width": 800,
                                    "height": 1000
                                }
                            }
                        ]
                    }
                ]
            }));
        }

        json!({
            "@context": "http://iiif.io/api/presentation/3/context.json",
            "id": manifest_id,
            "type": "Manifest",
            "label": manifest_label_val,
            "items": items
        })
        .to_string()
    }

    fn create_direct_image_manifest_json(
        manifest_id: &str,
        manifest_label_val: serde_json::Value,
        canvas_id_prefix: &str,
        canvas_label_val: serde_json::Value,
        image_direct_url: &str,
        num_canvases: usize,
    ) -> String {
        let mut items = Vec::new();
        for i in 0..num_canvases {
            items.push(json!({
                "id": format!("{}/canvas/{}", canvas_id_prefix, i),
                "type": "Canvas",
                "label": canvas_label_val.clone(),
                "height": 1000,
                "width": 800,
                "items": [
                    {
                        "id": format!("{}/page/{}/annotationpage", canvas_id_prefix, i),
                        "type": "AnnotationPage",
                        "items": [
                            {
                                "id": format!("{}/page/{}/annotation/1", canvas_id_prefix, i),
                                "type": "Annotation",
                                "motivation": "painting",
                                "body": {
                                    "id": image_direct_url,
                                    "type": "Image",
                                    "format": "image/jpeg",
                                    "width": 800,
                                    "height": 1000
                                }
                            }
                        ]
                    }
                ]
            }));
        }

        json!({
            "@context": "http://iiif.io/api/presentation/3/context.json",
            "id": manifest_id,
            "type": "Manifest",
            "label": manifest_label_val,
            "items": items
        })
        .to_string()
    }

    #[tokio::test]
    async fn test_parse_valid_manifest_multilingual_label() {
        let parser = IiifManifestBulkParser::new();
        let manifest_json = create_minimal_manifest_json(
            "http://example.com/manifest",
            json!({"en": ["My Book"], "fr": ["Mon Livre"]}),
            "http://example.com/manifest",
            json!({"en": ["Page Label"], "none": ["No Label"]}),
            "http://example.com/images/book1_page1",
            2,
        );

        let result = parser
            .parse(&manifest_json, Some("http://example.com/manifest"))
            .await
            .unwrap();
        assert_eq!(result.len(), 2);

        assert_eq!(
            result[0].download_url,
            "http://example.com/images/book1_page1/info.json"
        );
        assert_eq!(result[0].default_filename_stem, "My_Book_page_1");
        assert_eq!(result[0].template_vars["manifest_label"], "My Book");
        assert_eq!(result[0].template_vars["canvas_label"], "Page Label");
        assert_eq!(result[0].template_vars["page_number"], "1");
        assert_eq!(result[0].template_vars["total_pages"], "2");
        assert_eq!(result[0].template_vars["canvas_index"], "0");

        assert_eq!(
            result[1].download_url,
            "http://example.com/images/book1_page1/info.json"
        );
        assert_eq!(result[1].default_filename_stem, "My_Book_page_2");
        assert_eq!(result[1].template_vars["manifest_label"], "My Book");
        assert_eq!(result[1].template_vars["canvas_label"], "Page Label");
        assert_eq!(result[1].template_vars["page_number"], "2");
        assert_eq!(result[1].template_vars["total_pages"], "2");
        assert_eq!(result[1].template_vars["canvas_index"], "1");
    }

    #[tokio::test]
    async fn test_parse_manifest_with_none_labels() {
        let parser = IiifManifestBulkParser::new();
        let manifest_json = create_minimal_manifest_json(
            "http://example.com/manifest-none",
            json!({"none": ["Label in 'none'"]}),
            "http://example.com/manifest-none",
            json!({}),
            "http://example.com/images/none_page",
            1,
        );

        let result = parser
            .parse(&manifest_json, Some("http://example.com/manifest-none"))
            .await
            .unwrap();
        assert_eq!(result.len(), 1);

        assert_eq!(
            result[0].download_url,
            "http://example.com/images/none_page/info.json"
        );
        assert_eq!(result[0].default_filename_stem, "Label_in_none_page_1");
        assert_eq!(result[0].template_vars["manifest_label"], "Label in 'none'");
        assert_eq!(result[0].template_vars["canvas_label"], "None");
        assert_eq!(result[0].template_vars["page_number"], "1");
    }

    #[tokio::test]
    async fn test_parse_manifest_with_empty_string_labels() {
        let parser = IiifManifestBulkParser::new();
        let manifest_json = create_minimal_manifest_json(
            "http://example.com/manifest-empty",
            json!(""),
            "http://example.com/manifest-empty",
            json!(""),
            "http://example.com/images/empty_page",
            1,
        );

        let result = parser
            .parse(&manifest_json, Some("http://example.com/manifest-empty"))
            .await
            .unwrap();
        assert_eq!(result.len(), 1);

        assert_eq!(
            result[0].download_url,
            "http://example.com/images/empty_page/info.json"
        );
        assert_eq!(result[0].default_filename_stem, "manifest_page_1");
        assert_eq!(result[0].template_vars["manifest_label"], "");
        assert_eq!(result[0].template_vars["canvas_label"], "");
        assert_eq!(result[0].template_vars["page_number"], "1");
    }

    #[tokio::test]
    async fn test_parse_invalid_json() {
        let parser = IiifManifestBulkParser::new();
        let invalid_json = "{ \"id\": \"bad json";
        let result = parser.parse(invalid_json, None).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .starts_with("Failed to parse IIIF Manifest JSON:")
        );
    }

    #[tokio::test]
    async fn test_parse_manifest_no_items() {
        let parser = IiifManifestBulkParser::new();
        let manifest_json = json!({
            "@context": "http://iiif.io/api/presentation/3/context.json",
            "id": "http://example.com/manifest-no-items",
            "type": "Manifest",
            "label": {"en": ["No Items Manifest"]},
            "items": []
        })
        .to_string();

        let result = parser
            .parse(&manifest_json, Some("http://example.com/manifest-no-items"))
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_sanitize_filename_logic() {
        assert_eq!(sanitize_for_filename("My Awesome Book!"), "My_Awesome_Book");
        assert_eq!(
            sanitize_for_filename("  Leading and Trailing Spaces  "),
            "Leading_and_Trailing_Spaces"
        );
        assert_eq!(
            sanitize_for_filename("book_vol_1_part_2.pdf"),
            "book_vol_1_part_2pdf"
        );
        assert_eq!(
            sanitize_for_filename("!@#$%^&*()_+=-`~[]{}|\\:;\"'<>,.?/"),
            "-"
        );
        assert_eq!(sanitize_for_filename(""), "");
        assert_eq!(sanitize_for_filename("None"), "None");
    }

    #[tokio::test]
    async fn test_default_filename_stem_generation() {
        let m_label1 = "My Great Document";
        let page_num1 = 1;
        let expected_stem1 = format!("{}_page_{}", sanitize_for_filename(m_label1), page_num1);
        assert_eq!(expected_stem1, "My_Great_Document_page_1");

        let page_num2 = 2;
        let expected_stem2 = format!("manifest_page_{}", page_num2);
        assert_eq!(expected_stem2, "manifest_page_2");

        let page_num3 = 3;
        let expected_stem3 = format!("manifest_page_{}", page_num3);
        assert_eq!(expected_stem3, "manifest_page_3");

        let page_num4 = 4;
        let expected_stem4 = format!("manifest_page_{}", page_num4);
        assert_eq!(expected_stem4, "manifest_page_4");
    }

    #[tokio::test]
    async fn test_parse_direct_image_url_in_manifest() {
        let parser = IiifManifestBulkParser::new();
        let manifest_json = create_direct_image_manifest_json(
            "http://example.com/manifest-direct",
            json!({"en": ["Direct Image Book"]}),
            "http://example.com/manifest-direct",
            json!({"en": ["Direct Page"]}),
            "http://example.com/images/direct_image.png",
            1,
        );

        let result = parser
            .parse(&manifest_json, Some("http://example.com/manifest-direct"))
            .await
            .unwrap();
        assert_eq!(result.len(), 1);

        assert_eq!(
            result[0].download_url,
            "http://example.com/images/direct_image.png"
        );
        assert_eq!(result[0].default_filename_stem, "Direct_Image_Book_page_1");
        assert_eq!(
            result[0].template_vars["manifest_label"],
            "Direct Image Book"
        );
        assert_eq!(result[0].template_vars["canvas_label"], "Direct Page");
        assert_eq!(result[0].template_vars["page_number"], "1");
    }
}
