use crate::DatabaseError;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;

/// Enum represents a media type and can be used on a library or on a media.
/// When returned in a http response, the fields are lowercase.
#[derive(Copy, Serialize, Debug, Clone, Eq, PartialEq, Deserialize, Hash, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum MediaType {
    Movie,
    Tv,
    Episode,
}

impl fmt::Display for MediaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Movie => "movie",
                Self::Tv => "tv",
                Self::Episode => "episode",
            }
        )
    }
}

impl Default for MediaType {
    fn default() -> Self {
        Self::Movie
    }
}

/// Library struct which we can use to deserialize database queries into.
#[derive(Serialize, Deserialize, Clone)]
pub struct Library {
    /// unique id provided by postgres
    pub id: i64,
    /// unique name of the library
    pub name: String,

    /// a path on the filesystem that holds media. ie /home/user/media/movies
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<String>,

    /// Enum used to identify the media type that this library contains. At the
    /// moment only `movie` and `tv` are supported
    // TODO: support mixed content, music
    pub media_type: MediaType,
}

impl Library {
    /// Method returns all libraries that exist in the database in the form of a Vec.
    /// If no libraries are found the the Vec will just be empty.
    ///
    /// This method will not return the locations indexed for this library, if you need those you
    /// must query for them separately.
    pub async fn get_all(conn: &crate::DbConnection) -> Vec<Self> {
        sqlx::query!(r#"SELECT id, name, media_type as "media_type: MediaType" FROM library"#)
            .fetch_all(conn)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|x| Self {
                id: x.id,
                name: x.name,
                media_type: x.media_type,
                locations: vec![],
            })
            .collect()
    }

    pub async fn get_locations(
        conn: &crate::DbConnection,
        id: i64,
    ) -> Result<Vec<String>, DatabaseError> {
        Ok(sqlx::query_scalar!(
            "SELECT location FROM indexed_paths
            WHERE library_id = ?",
            id
        )
        .fetch_all(conn)
        .await?)
    }

    /// Method filters the database for a library with the id supplied and returns it.
    /// This method will also fetch the indexed locations for this library.
    ///
    /// # Arguments
    /// * `conn` - [diesel connection](crate::DbConnection)
    /// * `lib_id` - a integer that is the id of the library we are trying to query
    pub async fn get_one(conn: &crate::DbConnection, lib_id: i64) -> Result<Self, DatabaseError> {
        // NOTE: Create a transaction so we immediately lock the database.
        let _tx = conn.begin().await?;

        let library = sqlx::query!(
            r#"SELECT id, name, media_type as "media_type: MediaType" FROM library
            WHERE id = ?"#,
            lib_id
        )
        .fetch_one(conn)
        .await?;

        let locations = sqlx::query_scalar!(
            r#"SELECT location FROM indexed_paths
            WHERE library_id = ?"#,
            lib_id
        )
        .fetch_all(conn)
        .await?;

        Ok(Self {
            id: library.id,
            name: library.name,
            media_type: library.media_type,
            locations,
        })
    }

    /// Method filters the database for a library with the id supplied and deletes it.
    ///
    /// # Arguments
    /// * `conn` - [diesel connection](crate::DbConnection)
    /// * `lib_id` - a integer that is the id of the library we are trying to query
    pub async fn delete(
        conn: &crate::DbConnection,
        id_to_del: i64,
    ) -> Result<usize, DatabaseError> {
        Ok(sqlx::query!("DELETE FROM library WHERE id = ?", id_to_del)
            .execute(conn)
            .await?
            .rows_affected() as usize)
    }
}

/// InsertableLibrary struct, same as [`Library`](Library) but without the id field.
#[derive(Clone, Serialize, Deserialize)]
pub struct InsertableLibrary {
    pub name: String,
    pub locations: Vec<String>,
    pub media_type: MediaType,
}

impl InsertableLibrary {
    /// Method inserts a InsertableLibrary object into the database (makes a new library).
    ///
    /// # Arguments
    /// * `conn` - [diesel connection](crate::DbConnection)
    pub async fn insert(&self, conn: &crate::DbConnection) -> Result<i64, DatabaseError> {
        let tx = conn.begin().await?;
        let lib_id = sqlx::query!(
            r#"INSERT INTO library (name, media_type) VALUES ($1, $2)"#,
            self.name,
            self.media_type
        )
        .execute(conn)
        .await?
        .last_insert_rowid();

        for location in &self.locations {
            sqlx::query!(
                r#"INSERT into indexed_paths(location, library_id)
                VALUES ($1, $2)"#,
                location,
                lib_id
            )
            .execute(conn)
            .await?;
        }

        tx.commit().await?;

        Ok(lib_id)
    }
}
