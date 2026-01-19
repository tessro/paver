//! Build static documentation site from PAVED docs.
//!
//! This module implements the `paver build` command which generates a static
//! HTML site from the documentation files.

use anyhow::{Context, Result};
use pulldown_cmark::{Options, Parser, html};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{CONFIG_FILENAME, PaverConfig};

/// Arguments for the `paver build` command.
pub struct BuildArgs {
    /// Output directory for the built site.
    pub output: PathBuf,
}

/// Execute the `paver build` command.
pub fn execute(args: BuildArgs) -> Result<()> {
    let config = load_config()?;
    let docs_root = &config.docs.root;

    // Check if docs directory exists
    if !docs_root.exists() {
        anyhow::bail!(
            "documentation directory '{}' does not exist",
            docs_root.display()
        );
    }

    // Find the site source directory
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let site_source = find_site_source(&cwd)?;

    let output_dir = &args.output;

    // Clean output directory if it exists
    if output_dir.exists() {
        fs::remove_dir_all(output_dir)
            .with_context(|| format!("failed to clean output directory: {}", output_dir.display()))?;
    }

    // Create output directory
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create output directory: {}", output_dir.display()))?;

    // Step 1: Copy site source files (assets, index.html, etc.)
    copy_site_source(&site_source, output_dir)?;

    // Step 2: Copy and process paver docs
    let paved_docs_dest = output_dir.join("paved-docs");
    fs::create_dir_all(&paved_docs_dest)?;
    copy_and_process_docs(docs_root, &paved_docs_dest)?;

    // Step 3: Process user guide docs from site/docs
    let site_docs = site_source.join("docs");
    if site_docs.exists() {
        let docs_dest = output_dir.join("docs");
        fs::create_dir_all(&docs_dest)?;
        copy_and_process_docs(&site_docs, &docs_dest)?;
    }

    // Step 4: Build HTML from all markdown files
    build_html_files(output_dir)?;

    println!("Built site at: {}", output_dir.display());

    Ok(())
}

/// Load paver configuration from current directory or parents.
fn load_config() -> Result<PaverConfig> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;

    // Search for config file in current directory and parents
    let mut search_path = cwd.as_path();
    loop {
        let config_path = search_path.join(CONFIG_FILENAME);
        if config_path.exists() {
            return PaverConfig::load(&config_path);
        }

        match search_path.parent() {
            Some(parent) => search_path = parent,
            None => break,
        }
    }

    // No config found, use defaults
    Ok(PaverConfig::default())
}

/// Find the site source directory.
fn find_site_source(start: &Path) -> Result<PathBuf> {
    let mut search_path = start;
    loop {
        let site_path = search_path.join("site");
        if site_path.exists() && site_path.join("_layouts").exists() {
            return Ok(site_path);
        }

        match search_path.parent() {
            Some(parent) => search_path = parent,
            None => break,
        }
    }

    anyhow::bail!("could not find site/ directory with _layouts")
}

/// Copy site source files to output directory.
fn copy_site_source(source: &Path, dest: &Path) -> Result<()> {
    copy_dir_recursive(source, dest, &|path| {
        // Skip _layouts (we'll inline them), _config.yml (Jekyll-specific),
        // and markdown files in docs/ (we'll process them separately)
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('_') || name == "docs" {
            return false;
        }
        true
    })
}

/// Copy directory recursively with a filter.
fn copy_dir_recursive<F>(source: &Path, dest: &Path, filter: &F) -> Result<()>
where
    F: Fn(&Path) -> bool,
{
    if !filter(source) {
        return Ok(());
    }

    if source.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());
            copy_dir_recursive(&src_path, &dest_path, filter)?;
        }
    } else {
        fs::copy(source, dest)
            .with_context(|| format!("failed to copy {} to {}", source.display(), dest.display()))?;
    }

    Ok(())
}

/// Copy and process documentation files.
fn copy_and_process_docs(source: &Path, dest: &Path) -> Result<()> {
    if source.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());

            // Skip templates directory
            if src_path.is_dir() && src_path.file_name().is_some_and(|n| n == "templates") {
                continue;
            }

            copy_and_process_docs(&src_path, &dest_path)?;
        }
    } else if source.extension().is_some_and(|ext| ext == "md") {
        let content = fs::read_to_string(source)?;
        let processed = process_markdown(&content, source)?;
        fs::write(dest, processed)?;
    } else {
        fs::copy(source, dest)?;
    }

    Ok(())
}

/// Process a markdown file: add front matter and convert links.
fn process_markdown(content: &str, path: &Path) -> Result<String> {
    let mut result = content.to_string();

    // Convert .md links to directory links (for pretty URLs)
    // [text](./path/file.md) -> [text](./path/file/)
    result = convert_md_links(&result);

    // Add front matter if not present
    if !result.trim_start().starts_with("---") {
        let title = extract_title(&result).unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string()
        });

        let front_matter = format!("---\nlayout: doc\ntitle: \"{}\"\n---\n\n", title);
        result = format!("{}{}", front_matter, result);
    }

    Ok(result)
}

/// Convert .md links to directory links.
fn convert_md_links(content: &str) -> String {
    // Replace .md) with /) for simple links
    // Replace .md#anchor) with /#anchor) for links with anchors
    content
        .replace(".md)", "/)")
        .replace(".md#", "/#")
}

/// Extract the title from the first # heading.
fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            return Some(title.trim().to_string());
        }
    }
    None
}

/// Build HTML files from all markdown files in the output directory.
fn build_html_files(output_dir: &Path) -> Result<()> {
    // Load layouts
    let cwd = std::env::current_dir()?;
    let site_source = find_site_source(&cwd)?;
    let default_layout = fs::read_to_string(site_source.join("_layouts/default.html"))
        .context("failed to read default layout")?;
    let doc_layout = fs::read_to_string(site_source.join("_layouts/doc.html"))
        .context("failed to read doc layout")?;

    // Find and process all markdown and HTML files with front matter
    process_content_dir(output_dir, output_dir, &default_layout, &doc_layout)
}

/// Recursively process markdown and HTML files in a directory.
fn process_content_dir(
    dir: &Path,
    output_root: &Path,
    default_layout: &str,
    doc_layout: &str,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            process_content_dir(&path, output_root, default_layout, doc_layout)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            convert_md_to_html(&path, output_root, default_layout, doc_layout)?;
        } else if path.extension().is_some_and(|ext| ext == "html") {
            // Process HTML files that have Jekyll front matter
            process_html_file(&path, output_root, default_layout, doc_layout)?;
        }
    }

    Ok(())
}

/// Process an HTML file that may contain Jekyll front matter and variables.
fn process_html_file(
    html_path: &Path,
    output_root: &Path,
    default_layout: &str,
    doc_layout: &str,
) -> Result<()> {
    let content = fs::read_to_string(html_path)?;

    // Check if file has front matter
    if !content.trim_start().starts_with("---") {
        // No front matter, leave file as-is
        return Ok(());
    }

    // Parse front matter and content
    let (front_matter, html_content) = parse_front_matter(&content);

    // Determine layout
    let layout = front_matter
        .get("layout")
        .map(|s| s.as_str())
        .unwrap_or("default");

    let template = if layout == "doc" { doc_layout } else { default_layout };

    // Get title for page
    let title = front_matter
        .get("title")
        .cloned()
        .unwrap_or_else(|| "paver".to_string());

    // Apply template (the HTML content is already HTML, not markdown)
    let html = apply_template(template, &html_content, &title, output_root, html_path)?;

    // Write processed HTML file
    fs::write(html_path, html)?;

    Ok(())
}

/// Convert a markdown file to HTML.
fn convert_md_to_html(
    md_path: &Path,
    output_root: &Path,
    default_layout: &str,
    doc_layout: &str,
) -> Result<()> {
    let content = fs::read_to_string(md_path)?;

    // Parse front matter and content
    let (front_matter, markdown_content) = parse_front_matter(&content);

    // Convert markdown to HTML
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(&markdown_content, options);
    let mut html_content = String::new();
    html::push_html(&mut html_content, parser);

    // Determine layout
    let layout = front_matter
        .get("layout")
        .map(|s| s.as_str())
        .unwrap_or("default");

    let template = if layout == "doc" { doc_layout } else { default_layout };

    // Get title for page
    let title = front_matter
        .get("title")
        .cloned()
        .or_else(|| extract_title(&markdown_content))
        .unwrap_or_else(|| "paver".to_string());

    // Apply template
    let html = apply_template(template, &html_content, &title, output_root, md_path)?;

    // Write HTML file
    // For index.md, create index.html in same directory
    // For other files, create a directory with index.html for pretty URLs
    let file_stem = md_path.file_stem().and_then(|s| s.to_str()).unwrap_or("index");

    let html_path = if file_stem == "index" {
        md_path.with_extension("html")
    } else {
        // Create directory for pretty URLs: foo.md -> foo/index.html
        let dir = md_path.with_extension("");
        fs::create_dir_all(&dir)?;
        dir.join("index.html")
    };

    fs::write(&html_path, html)?;

    // Remove the original markdown file
    fs::remove_file(md_path)?;

    Ok(())
}

/// Parse front matter from markdown content.
fn parse_front_matter(content: &str) -> (std::collections::HashMap<String, String>, String) {
    let mut front_matter = std::collections::HashMap::new();
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return (front_matter, content.to_string());
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    if let Some(end_pos) = after_first.find("\n---") {
        let fm_content = &after_first[..end_pos];
        let markdown_content = &after_first[end_pos + 4..];

        // Parse simple YAML-like front matter
        for line in fm_content.lines() {
            let line = line.trim();
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_string();
                let value = value.trim().trim_matches('"').to_string();
                front_matter.insert(key, value);
            }
        }

        return (front_matter, markdown_content.trim_start().to_string());
    }

    (front_matter, content.to_string())
}

/// Apply template to HTML content.
fn apply_template(
    template: &str,
    content: &str,
    title: &str,
    output_root: &Path,
    current_file: &Path,
) -> Result<String> {
    // Calculate relative path to root for asset URLs
    let relative_depth = current_file
        .strip_prefix(output_root)
        .unwrap_or(current_file)
        .components()
        .count()
        .saturating_sub(1);

    let base_path = if relative_depth == 0 {
        ".".to_string()
    } else {
        std::iter::repeat("..").take(relative_depth).collect::<Vec<_>>().join("/")
    };

    let mut result = template.to_string();

    // Replace Jekyll-style template variables
    result = result.replace("{{ content }}", content);

    // Handle title with conditional
    let title_replacement = if title.is_empty() || title == "paver" {
        "paver".to_string()
    } else {
        format!("{} | paver", title)
    };
    // Remove Jekyll conditionals and replace with simple title
    result = replace_title_block(&result, &title_replacement);

    // Replace asset URLs
    result = result.replace("{{ '/assets/css/style.css' | relative_url }}", &format!("{}/assets/css/style.css", base_path));
    result = result.replace("{{ '/assets/images/favicon.svg' | relative_url }}", &format!("{}/assets/images/favicon.svg", base_path));
    result = result.replace("{{ '/' | relative_url }}", &format!("{}/", base_path));
    result = result.replace("{{ '/docs/' | relative_url }}", &format!("{}/docs/", base_path));
    result = result.replace("{{ '/paved-docs/' | relative_url }}", &format!("{}/paved-docs/", base_path));
    result = result.replace("{{ '/docs/manifesto/' | relative_url }}", &format!("{}/docs/manifesto/", base_path));
    result = result.replace("{{ '/docs/getting-started/' | relative_url }}", &format!("{}/docs/getting-started/", base_path));
    result = result.replace("{{ '/docs/commands/' | relative_url }}", &format!("{}/docs/commands/", base_path));
    result = result.replace("{{ '/paved-docs/manifesto/' | relative_url }}", &format!("{}/paved-docs/manifesto/", base_path));
    result = result.replace("{{ '/paved-docs/components/paver-cli/' | relative_url }}", &format!("{}/paved-docs/components/paver-cli/", base_path));
    result = result.replace("{{ '/paved-docs/components/validation-engine/' | relative_url }}", &format!("{}/paved-docs/components/validation-engine/", base_path));
    result = result.replace("{{ '/paved-docs/runbooks/add-command/' | relative_url }}", &format!("{}/paved-docs/runbooks/add-command/", base_path));
    result = result.replace("{{ '/paved-docs/runbooks/release/' | relative_url }}", &format!("{}/paved-docs/runbooks/release/", base_path));
    result = result.replace("{{ '/paved-docs/adrs/001-use-paved-framework/' | relative_url }}", &format!("{}/paved-docs/adrs/001-use-paved-framework/", base_path));
    result = result.replace("{{ '/paved-docs/adrs/002-use-rust/' | relative_url }}", &format!("{}/paved-docs/adrs/002-use-rust/", base_path));

    // Replace site variables
    result = result.replace("{{ site.title }}", "paver");
    result = result.replace("{{ site.description }}", "PAVED docs for the AI agent era");

    // Remove any remaining Jekyll conditionals
    result = remove_jekyll_conditionals(&result);

    Ok(result)
}

/// Replace the title block with proper title.
fn replace_title_block(content: &str, title: &str) -> String {
    // Replace: {% if page.title %}{{ page.title }} | {% endif %}{{ site.title }}
    let pattern = "{% if page.title %}{{ page.title }} | {% endif %}{{ site.title }}";
    content.replace(pattern, title)
}

/// Remove remaining Jekyll conditionals.
fn remove_jekyll_conditionals(content: &str) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'%') {
            // Start of Jekyll tag, skip until %}
            chars.next(); // consume %
            while let Some(c2) = chars.next() {
                if c2 == '%' && chars.peek() == Some(&'}') {
                    chars.next(); // consume }
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_title() {
        let content = "# My Title\n\nSome content.";
        assert_eq!(extract_title(content), Some("My Title".to_string()));

        let content = "No title here";
        assert_eq!(extract_title(content), None);
    }

    #[test]
    fn test_convert_md_links() {
        let content = "See [getting started](./getting-started.md) for more.";
        let result = convert_md_links(content);
        assert_eq!(result, "See [getting started](./getting-started/) for more.");

        // Test links with anchors
        let content_with_anchor = "See [section](./doc.md#section) for more.";
        let result_with_anchor = convert_md_links(content_with_anchor);
        assert_eq!(result_with_anchor, "See [section](./doc/#section) for more.");
    }

    #[test]
    fn test_parse_front_matter() {
        let content = "---\nlayout: doc\ntitle: \"Test\"\n---\n\n# Hello";
        let (fm, md) = parse_front_matter(content);
        assert_eq!(fm.get("layout"), Some(&"doc".to_string()));
        assert_eq!(fm.get("title"), Some(&"Test".to_string()));
        assert_eq!(md, "# Hello");
    }

    #[test]
    fn test_parse_front_matter_no_fm() {
        let content = "# Hello\n\nWorld";
        let (fm, md) = parse_front_matter(content);
        assert!(fm.is_empty());
        assert_eq!(md, content);
    }

    #[test]
    fn test_process_markdown() {
        let content = "# Test Doc\n\nSee [other](./other.md) for more.";
        let path = PathBuf::from("test.md");
        let result = process_markdown(content, &path).unwrap();

        assert!(result.starts_with("---"));
        assert!(result.contains("title: \"Test Doc\""));
        assert!(result.contains("./other/)"));
    }

    #[test]
    fn test_process_markdown_existing_front_matter() {
        let content = "---\nlayout: default\n---\n# Test";
        let path = PathBuf::from("test.md");
        let result = process_markdown(content, &path).unwrap();

        // Should not add duplicate front matter
        assert_eq!(result.matches("---").count(), 2);
    }

    #[test]
    fn test_remove_jekyll_conditionals() {
        // The function removes Jekyll tags but keeps content between them
        let content = "Hello {% if foo %}bar{% endif %} world";
        let result = remove_jekyll_conditionals(content);
        assert_eq!(result, "Hello bar world");

        // Test removing tags with nothing between them
        let content2 = "Before {% if x %}{% endif %} After";
        let result2 = remove_jekyll_conditionals(content2);
        assert_eq!(result2, "Before  After");
    }

    #[test]
    fn test_copy_and_process_docs() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        let dest = temp.path().join("dest");

        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("test.md"), "# Test\n\nContent").unwrap();

        copy_and_process_docs(&source, &dest).unwrap();

        let output = fs::read_to_string(dest.join("test.md")).unwrap();
        assert!(output.contains("layout: doc"));
        assert!(output.contains("title: \"Test\""));
    }
}
