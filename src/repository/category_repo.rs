use rusqlite::Connection;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::repository::SettingsRepo;
use crate::models::{
    category::{Category, NewCategory},
    decimal_to_f64,
};

pub struct CategoryRepo;

impl CategoryRepo {
    pub fn create(conn: &Connection, input: &NewCategory) -> Result<Category> {
        let id = Uuid::new_v4().to_string();
        let now = now_ts();

        conn.execute(
            "INSERT INTO categories
             (id, name, icon, color, budget_amount, parent_id, sort_order, is_system, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
            rusqlite::params![
                id,
                input.name,
                input.icon,
                input.color,
                input.budget_amount.map(decimal_to_f64),
                input.parent_id,
                input.sort_order,
                now,
            ],
        )?;

        Self::find_by_id(conn, &id)?.ok_or_else(|| AppError::NotFound(id))
    }

    pub fn create_system(conn: &Connection, input: &NewCategory) -> Result<Category> {
        let id = Uuid::new_v4().to_string();
        let now = now_ts();

        conn.execute(
            "INSERT INTO categories
             (id, name, icon, color, budget_amount, parent_id, sort_order, is_system, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
            rusqlite::params![
                id,
                input.name,
                input.icon,
                input.color,
                input.budget_amount.map(decimal_to_f64),
                input.parent_id,
                input.sort_order,
                now,
            ],
        )?;

        Self::find_by_id(conn, &id)?.ok_or_else(|| AppError::NotFound(id))
    }

    pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<Category>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, icon, color, budget_amount, parent_id, sort_order, is_system, created_at
             FROM categories WHERE id = ?1",
        )?;
        match stmt.query_row([id], Category::from_row) {
            Ok(c)                                     => Ok(Some(c)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e)                                    => Err(e.into()),
        }
    }

    pub fn list(conn: &Connection) -> Result<Vec<Category>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, icon, color, budget_amount, parent_id, sort_order, is_system, created_at
             FROM categories
             ORDER BY is_system DESC, sort_order ASC, name ASC",
        )?;
        let rows = stmt.query_map([], Category::from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn list_root(conn: &Connection) -> Result<Vec<Category>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, icon, color, budget_amount, parent_id, sort_order, is_system, created_at
             FROM categories
             WHERE parent_id IS NULL
             ORDER BY is_system DESC, sort_order ASC",
        )?;
        let rows = stmt.query_map([], Category::from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn list_children(conn: &Connection, parent_id: &str) -> Result<Vec<Category>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, icon, color, budget_amount, parent_id, sort_order, is_system, created_at
             FROM categories
             WHERE parent_id = ?1
             ORDER BY sort_order ASC",
        )?;
        let rows = stmt.query_map([parent_id], Category::from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn update(conn: &Connection, cat: &Category) -> Result<()> {
        conn.execute(
            "UPDATE categories
             SET name = ?1, icon = ?2, color = ?3, budget_amount = ?4, sort_order = ?5
             WHERE id = ?6 AND is_system = 0",
            rusqlite::params![
                cat.name,
                cat.icon,
                cat.color,
                cat.budget_f64(),
                cat.sort_order,
                cat.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete(conn: &Connection, id: &str) -> Result<()> {
        // Prevent deleting system categories or ones in use
        let in_use: i64 = conn.query_row(
            "SELECT COUNT(*) FROM transactions WHERE category_id = ?1",
            [id],
            |row| row.get(0),
        )?;
        if in_use > 0 {
            return Err(AppError::Validation(
                format!("Category '{id}' is used by {in_use} transaction(s) and cannot be deleted"),
            ));
        }

        let affected = conn.execute(
            "DELETE FROM categories WHERE id = ?1 AND is_system = 0",
            [id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("category '{id}' (or it is a system category)")));
        }
        SettingsRepo::clear_default_category_if(conn, id)?;
        Ok(())
    }

    /// Add every default category not there yet (matched by name), so defaults added in a
    /// later version reach existing databases too. System categories can't be deleted.
    pub fn seed_defaults(conn: &Connection) -> Result<()> {
        for cat in crate::models::category::default_categories() {
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM categories WHERE is_system = 1 AND name = ?1",
                [&cat.name],
                |row| row.get(0),
            )?;
            if !exists {
                Self::create_system(conn, &cat)?;
            }
        }
        Ok(())
    }
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeding_adds_missing_defaults_only_once() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let total = crate::models::category::default_categories().len();

        CategoryRepo::seed_defaults(&conn).unwrap();
        assert_eq!(CategoryRepo::list(&conn).unwrap().len(), total);

        // An older database missing a newer default gets just that one
        conn.execute("DELETE FROM categories WHERE name = 'Family Support'", []).unwrap();
        CategoryRepo::seed_defaults(&conn).unwrap();
        CategoryRepo::seed_defaults(&conn).unwrap();
        let names: Vec<String> = CategoryRepo::list(&conn).unwrap().into_iter().map(|c| c.name).collect();
        assert_eq!(names.len(), total);
        assert_eq!(names.iter().filter(|n| *n == "Family Support").count(), 1);
    }
}
