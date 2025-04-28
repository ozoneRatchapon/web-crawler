use quick_xml::{Reader, events::Event};
use spider::website::Website;
use std::collections::HashSet;
use std::fs::{self, create_dir_all};
use std::path::Path;

fn main() {
    // Example usage
    if let Err(e) = run_crawler("https://spider.cloud") {
        eprintln!("Crawler failed: {}", e);
    }
}

// Main crawler function
fn run_crawler(domain: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Normalize domain
    let domain = domain.trim_end_matches('/').to_string();

    // Step 1: Load robots.txt and get sitemap URLs
    let sitemap_urls = get_sitemap_urls_from_robots(&domain)?;

    // Step 2: Get all page URLs
    let page_urls = if !sitemap_urls.is_empty() {
        // Load sitemaps recursively
        get_all_page_urls_from_sitemaps(&sitemap_urls)?
    } else {
        // Step 3: Try direct sitemap.xml if no robots.txt
        let sitemap_url = format!("{}/sitemap.xml", domain);
        let sitemap_urls = vec![sitemap_url.clone()];
        let sitemap_pages = get_all_page_urls_from_sitemaps(&sitemap_urls)?;

        if !sitemap_pages.is_empty() {
            sitemap_pages
        } else {
            // Step 4: Fallback to native crawl
            native_crawl(&domain)?
        }
    };

    // Step 5-8: Load HTML, convert to Markdown, and save
    for url in page_urls {
        let html = load_html(&url, &domain)?;
        let markdown = html_to_markdown(&html);
        save_markdown(&url, &markdown)?;
    }

    Ok(())
}

// Step 1: Load robots.txt and extract sitemap URLs
fn get_sitemap_urls_from_robots(domain: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let robots_url = format!("{}/robots.txt", domain);
    let mut website = Website::new(&robots_url);
    website.configuration.depth = 0; // Only fetch robots.txt
    website.scrape();

    let mut sitemap_urls = Vec::new();
    if let Some(pages) = website.get_pages() {
        if let Some(page) = pages.first() {
            let content = page.get_html();
            sitemap_urls = parse_robots_txt(&content);
        }
    } else {
        eprintln!("Warning: Failed to fetch robots.txt for {}", robots_url);
    }
    Ok(sitemap_urls)
}

// Parse robots.txt to find sitemap URLs
fn parse_robots_txt(content: &str) -> Vec<String> {
    content
        .lines()
        .filter(|line| line.to_lowercase().starts_with("sitemap:"))
        .map(|line| {
            line.trim_start_matches(|c: char| c.is_whitespace() || c.to_ascii_lowercase() == 's')
                .trim()
                .to_string()
        })
        .collect()
}

// Step 2: Load sitemaps recursively and extract page URLs
fn get_all_page_urls_from_sitemaps(
    sitemap_urls: &[String],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut all_urls = HashSet::new();
    for sitemap_url in sitemap_urls {
        parse_sitemap_recursive(sitemap_url, &mut all_urls)?;
    }
    Ok(all_urls.into_iter().collect())
}

// Recursive sitemap parsing
fn parse_sitemap_recursive(
    sitemap_url: &str,
    all_urls: &mut HashSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut website = Website::new(sitemap_url);
    website.configuration.depth = 0;
    website.scrape();

    if let Some(pages) = website.get_pages() {
        if let Some(page) = pages.first() {
            let html = page.get_html();
            let mut reader = Reader::from_str(&html);

            let mut buf = Vec::new();
            let mut in_loc = false;
            let mut current_url = String::new();

            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Start(e)) => {
                        if e.name().as_ref() == b"loc" {
                            in_loc = true;
                        }
                    }
                    Ok(Event::Text(e)) => {
                        if in_loc {
                            current_url = e.unescape()?.to_string();
                        }
                    }
                    Ok(Event::End(e)) => {
                        if e.name().as_ref() == b"loc" && !current_url.is_empty() {
                            in_loc = false;
                            if current_url.ends_with(".xml") {
                                // Nested sitemap
                                parse_sitemap_recursive(&current_url, all_urls)?;
                            } else {
                                // Page URL
                                all_urls.insert(current_url.clone());
                            }
                            current_url.clear();
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(e) => return Err(Box::new(e)),
                    _ => {}
                }
                buf.clear();
            }
        }
    } else {
        eprintln!("Warning: Failed to fetch sitemap {}", sitemap_url);
    }

    Ok(())
}

// Step 4: Native crawl if no robots.txt or sitemap.xml
fn native_crawl(domain: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut website = Website::new(domain);
    website.configuration.depth = 3; // Example depth
    website.configuration.delay = 100; // 100ms delay
    // website.configuration.use_browser = get_fetch_mode(domain);
    website.crawl();

    let urls = website
        .get_links()
        .into_iter()
        .map(|link| link.to_string())
        .collect();
    Ok(urls)
}

// Step 5: Load HTML from a URL
fn load_html(url: &str, domain: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut website = Website::new(url);
    website.configuration.depth = 0;
    // website.configuration.use_browser = get_fetch_mode(domain);
    website.scrape();

    let pages = website
        .get_pages()
        .ok_or_else(|| format!("Failed to load HTML for {}", url))?;
    if let Some(page) = pages.first() {
        Ok(page.get_html())
    } else {
        Err(format!("No HTML content for {}", url).into())
    }
}

// Step 6: Determine fetch mode based on domain
fn get_fetch_mode(domain: &str) -> bool {
    // Hardcoded whitelist for SPA/SSR detection
    domain.contains("www.heygoody.com") // true for browser (SPA), false for HTTP (SSR)
}

// Step 7: Convert HTML to Markdown
fn html_to_markdown(html: &str) -> String {
    let mut markdown = String::new();
    let mut in_tag: Option<String> = None;
    let mut current_content = String::new();
    let mut i = 0;
    let chars: Vec<char> = html.chars().collect();

    while i < chars.len() {
        if chars[i] == '<' {
            // Process previous content
            if !current_content.trim().is_empty() {
                match in_tag.as_deref() {
                    Some("h1") => markdown.push_str(&format!("# {}\n\n", current_content.trim())),
                    Some("h2") => markdown.push_str(&format!("## {}\n\n", current_content.trim())),
                    Some("p") => markdown.push_str(&format!("{}\n\n", current_content.trim())),
                    Some("li") => markdown.push_str(&format!("- {}\n", current_content.trim())),
                    Some("a") => markdown.push_str(&format!(
                        "[{}]({})",
                        current_content.trim(),
                        current_content.trim()
                    )),
                    Some("img") => {
                        markdown.push_str(&format!("![Image]({})\n", current_content.trim()))
                    }
                    Some("strong") => markdown.push_str(&format!("**{}**", current_content.trim())),
                    Some("em") => markdown.push_str(&format!("*{}*", current_content.trim())),
                    Some("blockquote") => {
                        markdown.push_str(&format!("> {}\n\n", current_content.trim()))
                    }
                    _ => markdown.push_str(&format!("{}\n", current_content.trim())),
                }
            }
            current_content.clear();

            // Parse tag
            let mut tag = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '>' && chars[i] != ' ' {
                tag.push(chars[i]);
                i += 1;
            }
            while i < chars.len() && chars[i] != '>' {
                i += 1;
            }
            i += 1; // Move past '>'

            if tag.starts_with('/') {
                in_tag = None;
            } else {
                match tag.as_str() {
                    "h1" | "h2" | "p" | "li" | "a" | "img" | "strong" | "em" | "blockquote" => {
                        in_tag = Some(tag);
                    }
                    "br" => markdown.push_str("\n"),
                    "ul" | "ol" => markdown.push_str("\n"),
                    _ => in_tag = None,
                }
            }
        } else {
            current_content.push(chars[i]);
            i += 1;
        }
    }

    // Handle remaining content
    if !current_content.trim().is_empty() {
        markdown.push_str(&format!("{}\n", current_content.trim()));
    }

    markdown
}

// Step 8: Save Markdown to file
fn save_markdown(url: &str, markdown: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Create output directory
    let output_dir = "output";
    create_dir_all(output_dir)?;

    // Generate file name from URL
    let file_name = url
        .trim_end_matches('/')
        .split('/')
        .last()
        .unwrap_or("index")
        .to_string()
        + ".md";
    let file_path = Path::new(output_dir).join(file_name);

    // Save file
    fs::write(file_path, markdown)?;
    Ok(())
}
