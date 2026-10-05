//! Adoption transitions, after the command boundary has collected the spool.

use crate::scope::Scope;
use crate::store::{Result, Store};
use rusqlite::{params, TransactionBehavior};

pub fn set_adopted(store: &mut Store, scope: &Scope, adopted: bool) -> Result<()> {
    let tx = store.transaction(TransactionBehavior::Immediate)?;
    let key = scope
        .project_key
        .to_str()
        .ok_or(crate::store::Error::InvalidStateDirectory)?;
    let at = crate::save::now();
    if adopted {
        tx.execute("INSERT INTO projects(key,adopted,adopted_at) VALUES (?1,1,?2)
            ON CONFLICT(key) DO UPDATE SET adopted=1,adopted_at=excluded.adopted_at WHERE projects.adopted=0", params![key, at])?;
    } else {
        tx.execute(
            "UPDATE projects SET adopted=0,unadopted_at=?2 WHERE key=?1 AND adopted=1",
            params![key, at],
        )?;
    }
    tx.commit()?;
    Ok(())
}
