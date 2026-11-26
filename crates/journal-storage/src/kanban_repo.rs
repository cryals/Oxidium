use chrono::Utc;
use journal_domain::{
    Badge, Board, ChecklistItem, DomainError, KanbanColumn, RecordSource, SearchEntityType, Tag,
    Task, TaskPriority,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use uuid::Uuid;

use crate::db::Database;
use crate::error::{StorageError, StorageResult};
use crate::helpers::{
    bump_sync_revision, delete_search_entry, normalize_unicode, parse_dt, parse_opt_date,
    parse_opt_dt, parse_opt_uuid, parse_uuid, upsert_search_entry,
};

impl Database {
    // ========================================================================
    // Boards & Columns
    // ========================================================================

    pub fn list_boards(
        &self,
        profile_id: Uuid,
        include_archived: bool,
    ) -> StorageResult<Vec<Board>> {
        let conn = self.lock_conn()?;
        let sql = if include_archived {
            "SELECT id, profile_id, name, description, color, sort_order, is_archived, created_at, updated_at
             FROM boards WHERE profile_id = ?1 ORDER BY is_archived ASC, sort_order ASC, created_at ASC"
        } else {
            "SELECT id, profile_id, name, description, color, sort_order, is_archived, created_at, updated_at
             FROM boards WHERE profile_id = ?1 AND is_archived = 0 ORDER BY sort_order ASC, created_at ASC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![profile_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i32>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, pid, name, desc, color, ord, arch, ca, ua) = r?;
            out.push(Board {
                id: parse_uuid(&id)?,
                profile_id: parse_uuid(&pid)?,
                name,
                description: desc,
                color,
                sort_order: ord,
                is_archived: arch != 0,
                created_at: parse_dt(&ca)?,
                updated_at: parse_dt(&ua)?,
            });
        }
        Ok(out)
    }

    pub fn get_board(&self, id: Uuid) -> StorageResult<Option<Board>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, profile_id, name, description, color, sort_order, is_archived, created_at, updated_at
                 FROM boards WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i32>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((id, pid, name, desc, color, ord, arch, ca, ua)) => Ok(Some(Board {
                id: parse_uuid(&id)?,
                profile_id: parse_uuid(&pid)?,
                name,
                description: desc,
                color,
                sort_order: ord,
                is_archived: arch != 0,
                created_at: parse_dt(&ca)?,
                updated_at: parse_dt(&ua)?,
            })),
        }
    }

    pub fn insert_board(&self, board: &Board, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO boards (
                id, profile_id, name, description, color, sort_order, is_archived, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                board.id.to_string(),
                board.profile_id.to_string(),
                board.name,
                board.description,
                board.color,
                board.sort_order,
                if board.is_archived { 1 } else { 0 },
                board.created_at.to_rfc3339(),
                board.updated_at.to_rfc3339(),
            ],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_board(&self, board: &Board, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE boards SET name = ?2, description = ?3, color = ?4, sort_order = ?5, is_archived = ?6, updated_at = ?7
             WHERE id = ?1",
            params![
                board.id.to_string(),
                board.name,
                board.description,
                board.color,
                board.sort_order,
                if board.is_archived { 1 } else { 0 },
                board.updated_at.to_rfc3339(),
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "board".into(),
                id: board.id.to_string(),
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_columns(
        &self,
        board_id: Uuid,
        include_archived: bool,
    ) -> StorageResult<Vec<KanbanColumn>> {
        let conn = self.lock_conn()?;
        let sql = if include_archived {
            "SELECT id, board_id, name, color, sort_order, is_collapsed, is_archived,
                    wip_limit, is_completed_column, created_at, updated_at
             FROM kanban_columns WHERE board_id = ?1 ORDER BY is_archived ASC, sort_order ASC"
        } else {
            "SELECT id, board_id, name, color, sort_order, is_collapsed, is_archived,
                    wip_limit, is_completed_column, created_at, updated_at
             FROM kanban_columns WHERE board_id = ?1 AND is_archived = 0 ORDER BY sort_order ASC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![board_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<u32>>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, bid, name, color, ord, coll, arch, wip, comp, ca, ua) = r?;
            out.push(KanbanColumn {
                id: parse_uuid(&id)?,
                board_id: parse_uuid(&bid)?,
                name,
                color,
                sort_order: ord,
                is_collapsed: coll != 0,
                is_archived: arch != 0,
                wip_limit: wip,
                is_completed_column: comp != 0,
                created_at: parse_dt(&ca)?,
                updated_at: parse_dt(&ua)?,
            });
        }
        Ok(out)
    }

    pub fn get_column(&self, id: Uuid) -> StorageResult<Option<KanbanColumn>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, board_id, name, color, sort_order, is_collapsed, is_archived,
                        wip_limit, is_completed_column, created_at, updated_at
                 FROM kanban_columns WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i32>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Option<u32>>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((id, bid, name, color, ord, coll, arch, wip, comp, ca, ua)) => {
                Ok(Some(KanbanColumn {
                    id: parse_uuid(&id)?,
                    board_id: parse_uuid(&bid)?,
                    name,
                    color,
                    sort_order: ord,
                    is_collapsed: coll != 0,
                    is_archived: arch != 0,
                    wip_limit: wip,
                    is_completed_column: comp != 0,
                    created_at: parse_dt(&ca)?,
                    updated_at: parse_dt(&ua)?,
                }))
            }
        }
    }

    pub fn insert_column(&self, col: &KanbanColumn, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO kanban_columns (
                id, board_id, name, color, sort_order, is_collapsed, is_archived,
                wip_limit, is_completed_column, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                col.id.to_string(),
                col.board_id.to_string(),
                col.name,
                col.color,
                col.sort_order,
                if col.is_collapsed { 1 } else { 0 },
                if col.is_archived { 1 } else { 0 },
                col.wip_limit,
                if col.is_completed_column { 1 } else { 0 },
                col.created_at.to_rfc3339(),
                col.updated_at.to_rfc3339(),
            ],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_column(&self, col: &KanbanColumn, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE kanban_columns SET
                name = ?2, color = ?3, sort_order = ?4, is_collapsed = ?5,
                is_archived = ?6, wip_limit = ?7, is_completed_column = ?8, updated_at = ?9
             WHERE id = ?1",
            params![
                col.id.to_string(),
                col.name,
                col.color,
                col.sort_order,
                if col.is_collapsed { 1 } else { 0 },
                if col.is_archived { 1 } else { 0 },
                col.wip_limit,
                if col.is_completed_column { 1 } else { 0 },
                col.updated_at.to_rfc3339(),
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "kanban_column".into(),
                id: col.id.to_string(),
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn reorder_columns(
        &self,
        board_id: Uuid,
        ordered_ids: &[Uuid],
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let now = Utc::now().to_rfc3339();
        for (idx, id) in ordered_ids.iter().enumerate() {
            tx.execute(
                "UPDATE kanban_columns SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND board_id = ?4",
                params![idx as i32, now, id.to_string(), board_id.to_string()],
            )?;
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_column_explicit(
        &self,
        column_id: Uuid,
        move_tasks_to: Option<Uuid>,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let cid = column_id.to_string();
        if let Some(target_col) = move_tasks_to {
            tx.execute(
                "UPDATE tasks SET column_id = ?1, updated_at = ?2 WHERE column_id = ?3",
                params![target_col.to_string(), Utc::now().to_rfc3339(), cid],
            )?;
        } else {
            tx.execute(
                "DELETE FROM search_index WHERE entity_type = 'task' AND entity_id IN (SELECT id FROM tasks WHERE column_id = ?1)",
                params![cid],
            )?;
            tx.execute("DELETE FROM tasks WHERE column_id = ?1", params![cid])?;
        }
        let changed = tx.execute("DELETE FROM kanban_columns WHERE id = ?1", params![cid])?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "kanban_column".into(),
                id: cid,
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    // ========================================================================
    // Tasks, Checklists, Links
    // ========================================================================

    pub fn insert_task(&self, task: &Task, source: RecordSource) -> StorageResult<()> {
        self.insert_tasks_batch(std::slice::from_ref(task), source)
    }

    pub fn insert_tasks_batch(&self, tasks: &[Task], source: RecordSource) -> StorageResult<()> {
        if tasks.is_empty() {
            return Ok(());
        }
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        for t in tasks {
            tx.execute(
                "INSERT INTO tasks (
                    id, profile_id, board_id, column_id, subject_id, title, description,
                    priority, color, start_date, due_at, completed_at, notes, sort_order,
                    is_archived, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    t.id.to_string(),
                    t.profile_id.to_string(),
                    t.board_id.to_string(),
                    t.column_id.to_string(),
                    t.subject_id.map(|u| u.to_string()),
                    t.title,
                    t.description,
                    t.priority.as_i32(),
                    t.color,
                    t.start_date.map(|d| d.format("%Y-%m-%d").to_string()),
                    t.due_at.map(|d| d.to_rfc3339()),
                    t.completed_at.map(|d| d.to_rfc3339()),
                    t.notes,
                    t.sort_order,
                    if t.is_archived { 1 } else { 0 },
                    t.created_at.to_rfc3339(),
                    t.updated_at.to_rfc3339(),
                ],
            )?;
            for tag in &t.tags {
                tx.execute(
                    "INSERT OR IGNORE INTO task_tags (task_id, tag_id) VALUES (?1, ?2)",
                    params![t.id.to_string(), tag.id.to_string()],
                )?;
            }
            for badge in &t.badges {
                tx.execute(
                    "INSERT OR IGNORE INTO task_badges (task_id, badge_id) VALUES (?1, ?2)",
                    params![t.id.to_string(), badge.id.to_string()],
                )?;
            }
            for item in &t.checklist {
                tx.execute(
                    "INSERT INTO task_checklist_items (id, task_id, title, is_completed, sort_order, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        item.id.to_string(),
                        t.id.to_string(),
                        item.title,
                        if item.is_completed { 1 } else { 0 },
                        item.sort_order,
                        item.created_at.to_rfc3339(),
                        item.updated_at.to_rfc3339(),
                    ],
                )?;
            }
            upsert_search_entry(
                &tx,
                SearchEntityType::Task,
                t.id,
                t.profile_id,
                t.subject_id,
                t.due_at.map(|dt| dt.date_naive()),
                &t.title,
                &format!("{} {}", t.description, t.notes),
            )?;
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_task(&self, task: &Task, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE tasks SET
                board_id = ?2, column_id = ?3, subject_id = ?4, title = ?5, description = ?6,
                priority = ?7, color = ?8, start_date = ?9, due_at = ?10, completed_at = ?11,
                notes = ?12, sort_order = ?13, is_archived = ?14, updated_at = ?15
             WHERE id = ?1",
            params![
                task.id.to_string(),
                task.board_id.to_string(),
                task.column_id.to_string(),
                task.subject_id.map(|u| u.to_string()),
                task.title,
                task.description,
                task.priority.as_i32(),
                task.color,
                task.start_date.map(|d| d.format("%Y-%m-%d").to_string()),
                task.due_at.map(|d| d.to_rfc3339()),
                task.completed_at.map(|d| d.to_rfc3339()),
                task.notes,
                task.sort_order,
                if task.is_archived { 1 } else { 0 },
                task.updated_at.to_rfc3339(),
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "task".into(),
                id: task.id.to_string(),
            }));
        }
        upsert_search_entry(
            &tx,
            SearchEntityType::Task,
            task.id,
            task.profile_id,
            task.subject_id,
            task.due_at.map(|dt| dt.date_naive()),
            &task.title,
            &format!("{} {}", task.description, task.notes),
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_task(&self, task_id: Uuid, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        delete_search_entry(&tx, SearchEntityType::Task, task_id)?;
        let changed = tx.execute(
            "DELETE FROM tasks WHERE id = ?1",
            params![task_id.to_string()],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "task".into(),
                id: task_id.to_string(),
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_task(&self, id: Uuid) -> StorageResult<Option<Task>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, profile_id, board_id, column_id, subject_id, title, description,
                        priority, color, start_date, due_at, completed_at, notes, sort_order,
                        is_archived, created_at, updated_at
                 FROM tasks WHERE id = ?1",
                params![id.to_string()],
                Self::map_task_row,
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some(r) => {
                let mut tasks = vec![Self::decode_task_row(r)?];
                Self::hydrate_tasks(&conn, &mut tasks)?;
                Ok(tasks.into_iter().next())
            }
        }
    }

    pub fn list_tasks(
        &self,
        profile_id: Uuid,
        board_id: Option<Uuid>,
        column_id: Option<Uuid>,
        subject_id: Option<Uuid>,
        include_archived: bool,
    ) -> StorageResult<Vec<Task>> {
        let conn = self.lock_conn()?;
        let mut sql = String::from(
            "SELECT id, profile_id, board_id, column_id, subject_id, title, description,
                    priority, color, start_date, due_at, completed_at, notes, sort_order,
                    is_archived, created_at, updated_at
             FROM tasks WHERE profile_id = ?1",
        );
        let mut values: Vec<rusqlite::types::Value> = vec![profile_id.to_string().into()];
        let mut idx = 2;
        if let Some(bid) = board_id {
            sql.push_str(&format!(" AND board_id = ?{idx}"));
            values.push(bid.to_string().into());
            idx += 1;
        }
        if let Some(cid) = column_id {
            sql.push_str(&format!(" AND column_id = ?{idx}"));
            values.push(cid.to_string().into());
            idx += 1;
        }
        if let Some(sid) = subject_id {
            sql.push_str(&format!(" AND subject_id = ?{idx}"));
            values.push(sid.to_string().into());
        }
        if !include_archived {
            sql.push_str(" AND is_archived = 0");
        }
        sql.push_str(" ORDER BY sort_order ASC, created_at ASC");

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values), Self::map_task_row)?;
        let mut tasks = Vec::new();
        for r in rows {
            tasks.push(Self::decode_task_row(r?)?);
        }
        Self::hydrate_tasks(&conn, &mut tasks)?;
        Ok(tasks)
    }

    fn hydrate_tasks(conn: &Connection, tasks: &mut [Task]) -> StorageResult<()> {
        if tasks.is_empty() {
            return Ok(());
        }
        let mut index_by_id: HashMap<Uuid, usize> = HashMap::with_capacity(tasks.len());
        for (i, t) in tasks.iter().enumerate() {
            index_by_id.insert(t.id, i);
        }

        // Load tags for these tasks (grouped by profile)
        let profile_id = tasks[0].profile_id.to_string();
        {
            let mut stmt = conn.prepare(
                "SELECT tt.task_id, tg.id, tg.profile_id, tg.name, tg.color, tg.icon, tg.description
                 FROM task_tags tt
                 JOIN tags tg ON tg.id = tt.tag_id
                 WHERE tg.profile_id = ?1",
            )?;
            let rows = stmt.query_map(params![profile_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })?;
            for r in rows {
                let (tid, id, pid, name, color, icon, desc) = r?;
                if let Ok(task_uuid) = parse_uuid(&tid)
                    && let Some(&idx) = index_by_id.get(&task_uuid)
                {
                    tasks[idx].tags.push(Tag {
                        id: parse_uuid(&id)?,
                        profile_id: parse_uuid(&pid)?,
                        name,
                        color,
                        icon,
                        description: desc,
                    });
                }
            }
        }
        {
            let mut stmt = conn.prepare(
                "SELECT tb.task_id, b.id, b.profile_id, b.name, b.color, b.icon, b.description
                 FROM task_badges tb
                 JOIN badges b ON b.id = tb.badge_id
                 WHERE b.profile_id = ?1",
            )?;
            let rows = stmt.query_map(params![profile_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })?;
            for r in rows {
                let (tid, id, pid, name, color, icon, desc) = r?;
                if let Ok(task_uuid) = parse_uuid(&tid)
                    && let Some(&idx) = index_by_id.get(&task_uuid)
                {
                    tasks[idx].badges.push(Badge {
                        id: parse_uuid(&id)?,
                        profile_id: parse_uuid(&pid)?,
                        name,
                        color,
                        icon,
                        description: desc,
                    });
                }
            }
        }
        {
            let mut stmt = conn.prepare(
                "SELECT c.id, c.task_id, c.title, c.is_completed, c.sort_order, c.created_at, c.updated_at
                 FROM task_checklist_items c
                 JOIN tasks t ON t.id = c.task_id
                 WHERE t.profile_id = ?1
                 ORDER BY c.sort_order ASC",
            )?;
            let rows = stmt.query_map(params![profile_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i32>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })?;
            for r in rows {
                let (id, tid, title, done, ord, ca, ua) = r?;
                if let Ok(task_uuid) = parse_uuid(&tid)
                    && let Some(&idx) = index_by_id.get(&task_uuid)
                {
                    tasks[idx].checklist.push(ChecklistItem {
                        id: parse_uuid(&id)?,
                        task_id: task_uuid,
                        title,
                        is_completed: done != 0,
                        sort_order: ord,
                        created_at: parse_dt(&ca)?,
                        updated_at: parse_dt(&ua)?,
                    });
                }
            }
        }
        {
            let mut stmt = conn.prepare(
                "SELECT l.task_id, l.grade_id
                 FROM task_grade_links l
                 JOIN tasks t ON t.id = l.task_id
                 WHERE t.profile_id = ?1",
            )?;
            let rows = stmt.query_map(params![profile_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for r in rows {
                let (tid, gid) = r?;
                if let Ok(task_uuid) = parse_uuid(&tid)
                    && let Some(&idx) = index_by_id.get(&task_uuid)
                {
                    tasks[idx].linked_grade_ids.push(parse_uuid(&gid)?);
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    fn map_task_row(
        row: &rusqlite::Row<'_>,
    ) -> rusqlite::Result<(
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        i32,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        i32,
        i64,
        String,
        String,
    )> {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
            row.get(8)?,
            row.get(9)?,
            row.get(10)?,
            row.get(11)?,
            row.get(12)?,
            row.get(13)?,
            row.get(14)?,
            row.get(15)?,
            row.get(16)?,
        ))
    }

    #[allow(clippy::type_complexity)]
    fn decode_task_row(
        raw: (
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            i32,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            i32,
            i64,
            String,
            String,
        ),
    ) -> StorageResult<Task> {
        let (
            id,
            pid,
            bid,
            cid,
            sid,
            title,
            desc,
            prio,
            color,
            sd,
            due,
            comp,
            notes,
            ord,
            arch,
            ca,
            ua,
        ) = raw;
        Ok(Task {
            id: parse_uuid(&id)?,
            profile_id: parse_uuid(&pid)?,
            board_id: parse_uuid(&bid)?,
            column_id: parse_uuid(&cid)?,
            subject_id: parse_opt_uuid(sid)?,
            title,
            description: desc,
            priority: TaskPriority::from_i32(prio),
            color,
            start_date: parse_opt_date(sd)?,
            due_at: parse_opt_dt(due)?,
            completed_at: parse_opt_dt(comp)?,
            notes,
            sort_order: ord,
            is_archived: arch != 0,
            created_at: parse_dt(&ca)?,
            updated_at: parse_dt(&ua)?,
            tags: Vec::new(),
            badges: Vec::new(),
            checklist: Vec::new(),
            linked_grade_ids: Vec::new(),
        })
    }

    // ========================================================================
    // Tags, Badges, Checklist Items & Grade Links
    // ========================================================================

    pub fn list_tags(&self, profile_id: Uuid) -> StorageResult<Vec<Tag>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, name, color, icon, description FROM tags WHERE profile_id = ?1 ORDER BY name ASC",
        )?;
        let rows = stmt.query_map(params![profile_id.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, pid, name, color, icon, desc) = r?;
            out.push(Tag {
                id: parse_uuid(&id)?,
                profile_id: parse_uuid(&pid)?,
                name,
                color,
                icon,
                description: desc,
            });
        }
        Ok(out)
    }

    pub fn find_tag_by_name(&self, profile_id: Uuid, name: &str) -> StorageResult<Option<Tag>> {
        let norm = normalize_unicode(name);
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, profile_id, name, color, icon, description FROM tags
                 WHERE profile_id = ?1 AND name_normalized = ?2",
                params![profile_id.to_string(), norm],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((id, pid, name, color, icon, desc)) => Ok(Some(Tag {
                id: parse_uuid(&id)?,
                profile_id: parse_uuid(&pid)?,
                name,
                color,
                icon,
                description: desc,
            })),
        }
    }

    pub fn insert_tag(&self, tag: &Tag, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let norm = normalize_unicode(&tag.name);
        tx.execute(
            "INSERT INTO tags (id, profile_id, name, name_normalized, color, icon, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                tag.id.to_string(),
                tag.profile_id.to_string(),
                tag.name,
                norm,
                tag.color,
                tag.icon,
                tag.description,
            ],
        )?;
        upsert_search_entry(
            &tx,
            SearchEntityType::Tag,
            tag.id,
            tag.profile_id,
            None,
            None,
            &tag.name,
            &tag.description,
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_tag(&self, tag: &Tag, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let norm = normalize_unicode(&tag.name);
        let changed = tx.execute(
            "UPDATE tags SET name = ?2, name_normalized = ?3, color = ?4, icon = ?5, description = ?6
             WHERE id = ?1",
            params![
                tag.id.to_string(),
                tag.name,
                norm,
                tag.color,
                tag.icon,
                tag.description,
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "tag".into(),
                id: tag.id.to_string(),
            }));
        }
        upsert_search_entry(
            &tx,
            SearchEntityType::Tag,
            tag.id,
            tag.profile_id,
            None,
            None,
            &tag.name,
            &tag.description,
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn assign_tag(
        &self,
        task_id: Uuid,
        tag_id: Uuid,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO task_tags (task_id, tag_id) VALUES (?1, ?2)",
            params![task_id.to_string(), tag_id.to_string()],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_tag(
        &self,
        task_id: Uuid,
        tag_id: Uuid,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM task_tags WHERE task_id = ?1 AND tag_id = ?2",
            params![task_id.to_string(), tag_id.to_string()],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_badges(&self, profile_id: Uuid) -> StorageResult<Vec<Badge>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, name, color, icon, description FROM badges WHERE profile_id = ?1 ORDER BY name ASC",
        )?;
        let rows = stmt.query_map(params![profile_id.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, pid, name, color, icon, desc) = r?;
            out.push(Badge {
                id: parse_uuid(&id)?,
                profile_id: parse_uuid(&pid)?,
                name,
                color,
                icon,
                description: desc,
            });
        }
        Ok(out)
    }

    pub fn find_badge_by_name(&self, profile_id: Uuid, name: &str) -> StorageResult<Option<Badge>> {
        let norm = normalize_unicode(name);
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, profile_id, name, color, icon, description FROM badges
                 WHERE profile_id = ?1 AND name_normalized = ?2",
                params![profile_id.to_string(), norm],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((id, pid, name, color, icon, desc)) => Ok(Some(Badge {
                id: parse_uuid(&id)?,
                profile_id: parse_uuid(&pid)?,
                name,
                color,
                icon,
                description: desc,
            })),
        }
    }

    pub fn insert_badge(&self, badge: &Badge, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let norm = normalize_unicode(&badge.name);
        tx.execute(
            "INSERT INTO badges (id, profile_id, name, name_normalized, color, icon, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                badge.id.to_string(),
                badge.profile_id.to_string(),
                badge.name,
                norm,
                badge.color,
                badge.icon,
                badge.description,
            ],
        )?;
        upsert_search_entry(
            &tx,
            SearchEntityType::Badge,
            badge.id,
            badge.profile_id,
            None,
            None,
            &badge.name,
            &badge.description,
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn assign_badge(
        &self,
        task_id: Uuid,
        badge_id: Uuid,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO task_badges (task_id, badge_id) VALUES (?1, ?2)",
            params![task_id.to_string(), badge_id.to_string()],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_badge(
        &self,
        task_id: Uuid,
        badge_id: Uuid,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM task_badges WHERE task_id = ?1 AND badge_id = ?2",
            params![task_id.to_string(), badge_id.to_string()],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn insert_checklist_item(
        &self,
        item: &ChecklistItem,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO task_checklist_items (id, task_id, title, is_completed, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                item.id.to_string(),
                item.task_id.to_string(),
                item.title,
                if item.is_completed { 1 } else { 0 },
                item.sort_order,
                item.created_at.to_rfc3339(),
                item.updated_at.to_rfc3339(),
            ],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_checklist_item(
        &self,
        item: &ChecklistItem,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE task_checklist_items SET title = ?2, is_completed = ?3, sort_order = ?4, updated_at = ?5
             WHERE id = ?1",
            params![
                item.id.to_string(),
                item.title,
                if item.is_completed { 1 } else { 0 },
                item.sort_order,
                item.updated_at.to_rfc3339(),
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "checklist_item".into(),
                id: item.id.to_string(),
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_checklist_item(&self, id: Uuid) -> StorageResult<Option<ChecklistItem>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, task_id, title, is_completed, sort_order, created_at, updated_at
                 FROM task_checklist_items WHERE id = ?1",
                params![id.to_string()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, i32>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((id, tid, title, done, ord, ca, ua)) => Ok(Some(ChecklistItem {
                id: parse_uuid(&id)?,
                task_id: parse_uuid(&tid)?,
                title,
                is_completed: done != 0,
                sort_order: ord,
                created_at: parse_dt(&ca)?,
                updated_at: parse_dt(&ua)?,
            })),
        }
    }

    pub fn delete_checklist_item(&self, id: Uuid, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "DELETE FROM task_checklist_items WHERE id = ?1",
            params![id.to_string()],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "checklist_item".into(),
                id: id.to_string(),
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn link_task_grade(
        &self,
        task_id: Uuid,
        grade_id: Uuid,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO task_grade_links (task_id, grade_id, created_at) VALUES (?1, ?2, ?3)",
            params![task_id.to_string(), grade_id.to_string(), Utc::now().to_rfc3339()],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn unlink_task_grade(
        &self,
        task_id: Uuid,
        grade_id: Uuid,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM task_grade_links WHERE task_id = ?1 AND grade_id = ?2",
            params![task_id.to_string(), grade_id.to_string()],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }
}
