use spider::tokio;
use spider::website::Website;

#[tokio::main]
async fn main() {
    let url = "https://choosealicense.com";
    let mut website = Website::new(&url);
    website.crawl().await;

    for link in website.get_links() {
        println!("- {:?}", link.as_ref());
    }
}
