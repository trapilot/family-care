//! JSON file "database" for imported statements (`statements.json` in the app data folder).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::{AppError, Result};
use crate::models::{Statement, StatementFile};

/// Serializes every read-modify-write so two imports finishing together never lose a write
static LOCK: Mutex<()> = Mutex::new(());

fn path() -> PathBuf {
    crate::db::data_dir().join("statements.json")
}

/// Read the file at `path`; a missing file is an empty `StatementFile`.
pub(crate) fn load_from(path: &Path) -> Result<StatementFile> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StatementFile::default()),
        Err(e) => Err(e.into()),
    }
}

/// Atomic save: write a `.tmp` sibling, then rename over the target.
pub(crate) fn save_to(path: &Path, file: &StatementFile) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, serde_json::to_vec_pretty(file)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load() -> Result<StatementFile> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    load_from(&path())
}

pub fn add(statement: Statement) -> Result<()> {
    add_to(&path(), statement)
}

pub fn delete(id: &str) -> Result<()> {
    delete_from(&path(), id)
}

/// Remove every imported statement by deleting the JSON file (photos are never stored).
pub fn clear() -> Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match std::fs::remove_file(path()) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

pub(crate) fn add_to(path: &Path, statement: Statement) -> Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = load_from(path)?;
    let existing: HashSet<String> = file
        .statements
        .iter()
        .flat_map(|s| s.transactions.iter().map(|t| super::StatementService::unique_key(s, t)))
        .collect();
    let all_known = !statement.transactions.is_empty()
        && statement
            .transactions
            .iter()
            .all(|t| existing.contains(&super::StatementService::unique_key(&statement, t)));
    if all_known {
        return Err(AppError::Validation("This statement page was already imported".into()));
    }
    file.statements.push(statement);
    save_to(path, &file)
}

pub(crate) fn delete_from(path: &Path, id: &str) -> Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = load_from(path)?;
    file.statements.retain(|s| s.id != id);
    save_to(path, &file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::StatementTxn;
    use rust_decimal::Decimal;

    fn txn(reference: &str) -> StatementTxn {
        StatementTxn {
            date: None,
            counterparty_name: None,
            counterparty_account: None,
            counterparty_bank: None,
            details: String::new(),
            reference: Some(reference.into()),
            debit: Decimal::from(5),
            credit: Decimal::ZERO,
            fee: Decimal::ZERO,
            tax: Decimal::ZERO,
            balance: None,
            needs_review: false,
        }
    }

    fn stmt(refs: &[&str]) -> Statement {
        Statement {
            id: uuid::Uuid::new_v4().to_string(),
            imported_at: 0,
            account_no: Some("123".into()),
            account_name: None,
            period_from: None,
            period_to: None,
            page: None,
            opening_balance: None,
            transactions: refs.iter().map(|r| txn(r)).collect(),
            warnings: vec![],
        }
    }

    #[test]
    fn add_delete_and_duplicates() {
        let path = std::env::temp_dir().join(format!("statements-test-{}.json", uuid::Uuid::new_v4()));
        assert!(load_from(&path).unwrap().statements.is_empty());

        let a = stmt(&["A", "B"]);
        let a_id = a.id.clone();
        add_to(&path, a).unwrap();
        // Same page again is rejected, a partial overlap is accepted
        assert!(matches!(add_to(&path, stmt(&["A", "B"])), Err(AppError::Validation(_))));
        add_to(&path, stmt(&["B", "C"])).unwrap();
        assert_eq!(load_from(&path).unwrap().statements.len(), 2);

        delete_from(&path, &a_id).unwrap();
        assert_eq!(load_from(&path).unwrap().statements.len(), 1);
        let _ = std::fs::remove_file(&path);
    }
}
