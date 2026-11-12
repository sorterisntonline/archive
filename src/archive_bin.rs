use sorter_archive::{archive, archive_state, archive_store, server};

fn main() {
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into());
    let db_path = format!("{}/archive.redb", data_dir);
    let needs_import = !std::path::Path::new(&db_path).exists()
        || std::env::var("ARCHIVE_IMPORT").as_deref() == Ok("1");
    let db = archive::open_store(&db_path);
    if needs_import {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:temp@localhost:15455/sorter".into());
        let mut client = archive_state::connect(&url);
        let report = archive::import(&db, &mut client);
        eprintln!(
            "imported: users={} items={} tags={} members={} votes={} dropped_items={} dropped_votes={}",
            report.users,
            report.items,
            report.tags,
            report.members,
            report.votes,
            report.dropped_items,
            report.dropped_votes
        );
    }
    let state = server::build_state(db);
    let app = server::app(state);
    let addr = server::listen_addr();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("bind");
        axum::serve(listener, app).await.expect("serve");
    });
}
