use rusqlite::Connection;

pub fn init_db() -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open("./data/aau_ajet.db")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS submissions (
                id INTEGER PRIMARY KEY,
                full_name TEXT NOT NULL,
                email TEXT NOT NULL,
                phone TEXT NOT NULL,
                title TEXT NOT NULL,
                abstract_text TEXT NOT NULL,
                pdf_url TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS journals (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            authors TEXT NOT NULL,
            abstract_text TEXT NOT NULL,
            keywords TEXT NOT NULL,
            volume_number INTEGER NOT NULL,
            issue_number INTEGER NOT NULL,
            pages TEXT NOT NULL,
            publication_date DATETIME NOT NULL,
            pdf_url TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS admins (
               id            INTEGER PRIMARY KEY AUTOINCREMENT,
               email         TEXT NOT NULL UNIQUE,
               password_hash TEXT NOT NULL,
               created_at    DATETIME DEFAULT CURRENT_TIMESTAMP
           )",
        [],
    )?;

    Ok(conn)
}
