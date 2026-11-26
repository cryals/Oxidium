use chrono::{DateTime, Duration, NaiveDate, Utc};
use journal_domain::{
    Badge, Board, ChecklistItem, DomainError, KanbanColumn, MAX_TEXT_LEN, MAX_TITLE_LEN,
    RecordSource, Reminder, ReminderOffset, Tag, Task, TaskPriority, validate_batch_size,
    validate_hex_color, validate_non_empty, validate_optional_text,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::AppResult;
use crate::i18n;
use crate::services::AppServices;
use crate::services::ops::AuditMeta;
use crate::undo::UndoCommand;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateBoardInput {
    pub profile_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub with_default_columns: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateBoardInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub is_archived: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateColumnInput {
    pub board_id: Uuid,
    pub name: String,
    pub color: Option<String>,
    pub wip_limit: Option<u32>,
    pub is_completed_column: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateColumnInput {
    pub name: Option<String>,
    pub color: Option<String>,
    pub wip_limit: Option<Option<u32>>,
    pub is_collapsed: Option<bool>,
    pub is_archived: Option<bool>,
    pub is_completed_column: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTaskInput {
    pub profile_id: Uuid,
    pub board_id: Option<Uuid>,
    pub column_id: Option<Uuid>,
    pub subject_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<TaskPriority>,
    pub color: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub due_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub tag_names: Vec<String>,
    pub badge_names: Vec<String>,
    pub checklist_titles: Vec<String>,
    pub reminder_offsets: Vec<ReminderOffset>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateTaskInput {
    pub column_id: Option<Uuid>,
    pub subject_id: Option<Option<Uuid>>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<TaskPriority>,
    pub color: Option<Option<String>>,
    pub start_date: Option<Option<NaiveDate>>,
    pub due_at: Option<Option<DateTime<Utc>>>,
    pub notes: Option<String>,
    pub sort_order: Option<i32>,
    pub is_archived: Option<bool>,
}

impl AppServices {
    // ========================================================================
    // Boards & Columns
    // ========================================================================

    pub fn list_boards(&self, profile_id: Uuid, include_archived: bool) -> AppResult<Vec<Board>> {
        Ok(self.db.list_boards(profile_id, include_archived)?)
    }

    pub fn get_board(&self, board_id: Uuid) -> AppResult<Board> {
        self.db.get_board(board_id)?.ok_or_else(|| {
            DomainError::NotFound {
                entity: "board".into(),
                id: board_id.to_string(),
            }
            .into()
        })
    }

    pub fn create_board(
        &self,
        input: CreateBoardInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Board> {
        let _ = self.get_profile(input.profile_id)?;
        let name = validate_non_empty("name", &input.name, MAX_TITLE_LEN)?;
        let color = validate_hex_color("color", input.color.as_deref().unwrap_or("#445E91"))?;
        let existing = self.db.list_boards(input.profile_id, true)?;
        let next_order = existing.iter().map(|b| b.sort_order).max().unwrap_or(-1) + 1;
        let now = Utc::now();
        let board = Board {
            id: Uuid::new_v4(),
            profile_id: input.profile_id,
            name,
            description: validate_optional_text(
                "description",
                input.description.as_deref().unwrap_or(""),
                MAX_TEXT_LEN,
            )?,
            color,
            sort_order: next_order,
            is_archived: false,
            created_at: now,
            updated_at: now,
        };
        self.db.insert_board(&board, source)?;

        if input.with_default_columns.unwrap_or(true) {
            let lang = self
                .db
                .load_settings()
                .map(|s| s.language)
                .unwrap_or_default();
            for (idx, (cname, ccolor, done)) in
                i18n::default_column_names(lang).into_iter().enumerate()
            {
                let col = KanbanColumn {
                    id: Uuid::new_v4(),
                    board_id: board.id,
                    name: cname.into(),
                    color: ccolor.into(),
                    sort_order: idx as i32,
                    is_collapsed: false,
                    is_archived: false,
                    wip_limit: None,
                    is_completed_column: done,
                    created_at: now,
                    updated_at: now,
                };
                self.db.insert_column(&col, source)?;
            }
        }
        self.log_activity(
            source,
            audit,
            Some(board.profile_id),
            "board",
            Some(board.id),
            "create",
            format!("Создана Kanban-доска «{}»", board.name),
            None,
        );
        self.sync.notify_local_change(source, "board");
        Ok(board)
    }

    pub fn update_board(
        &self,
        board_id: Uuid,
        input: UpdateBoardInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Board> {
        let mut board = self.get_board(board_id)?;
        if let Some(name) = input.name {
            board.name = validate_non_empty("name", &name, MAX_TITLE_LEN)?;
        }
        if let Some(desc) = input.description {
            board.description = validate_optional_text("description", &desc, MAX_TEXT_LEN)?;
        }
        if let Some(color) = input.color {
            board.color = validate_hex_color("color", &color)?;
        }
        if let Some(arch) = input.is_archived {
            board.is_archived = arch;
        }
        board.updated_at = Utc::now();
        self.db.update_board(&board, source)?;
        self.log_activity(
            source,
            audit,
            Some(board.profile_id),
            "board",
            Some(board.id),
            "update",
            format!("Обновлена Kanban-доска «{}»", board.name),
            None,
        );
        self.sync.notify_local_change(source, "board");
        Ok(board)
    }

    pub fn list_columns(
        &self,
        board_id: Uuid,
        include_archived: bool,
    ) -> AppResult<Vec<KanbanColumn>> {
        Ok(self.db.list_columns(board_id, include_archived)?)
    }

    pub fn create_column(
        &self,
        input: CreateColumnInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<KanbanColumn> {
        let board = self.get_board(input.board_id)?;
        let name = validate_non_empty("name", &input.name, MAX_TITLE_LEN)?;
        let color = validate_hex_color("color", input.color.as_deref().unwrap_or("#64748B"))?;
        let existing = self.db.list_columns(board.id, true)?;
        let next_order = existing.iter().map(|c| c.sort_order).max().unwrap_or(-1) + 1;
        let now = Utc::now();
        let col = KanbanColumn {
            id: Uuid::new_v4(),
            board_id: board.id,
            name,
            color,
            sort_order: next_order,
            is_collapsed: false,
            is_archived: false,
            wip_limit: input.wip_limit.filter(|&w| w > 0),
            is_completed_column: input.is_completed_column.unwrap_or(false),
            created_at: now,
            updated_at: now,
        };
        self.db.insert_column(&col, source)?;
        self.log_activity(
            source,
            audit,
            Some(board.profile_id),
            "kanban_column",
            Some(col.id),
            "create",
            format!("Создана колонка «{}»", col.name),
            None,
        );
        self.sync.notify_local_change(source, "kanban_column");
        Ok(col)
    }

    pub fn update_column(
        &self,
        column_id: Uuid,
        input: UpdateColumnInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<KanbanColumn> {
        let mut col = self
            .db
            .get_column(column_id)?
            .ok_or_else(|| DomainError::NotFound {
                entity: "kanban_column".into(),
                id: column_id.to_string(),
            })?;
        let board = self.get_board(col.board_id)?;
        if let Some(name) = input.name {
            col.name = validate_non_empty("name", &name, MAX_TITLE_LEN)?;
        }
        if let Some(color) = input.color {
            col.color = validate_hex_color("color", &color)?;
        }
        if let Some(wip) = input.wip_limit {
            col.wip_limit = wip.filter(|&w| w > 0);
        }
        if let Some(coll) = input.is_collapsed {
            col.is_collapsed = coll;
        }
        if let Some(arch) = input.is_archived {
            col.is_archived = arch;
        }
        if let Some(done) = input.is_completed_column {
            col.is_completed_column = done;
        }
        col.updated_at = Utc::now();
        self.db.update_column(&col, source)?;
        self.log_activity(
            source,
            audit,
            Some(board.profile_id),
            "kanban_column",
            Some(col.id),
            "update",
            format!("Обновлена колонка «{}»", col.name),
            None,
        );
        self.sync.notify_local_change(source, "kanban_column");
        Ok(col)
    }

    pub fn move_column(
        &self,
        board_id: Uuid,
        column_id: Uuid,
        target_index: usize,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Vec<KanbanColumn>> {
        let board = self.get_board(board_id)?;
        let mut cols = self.db.list_columns(board_id, false)?;
        let Some(pos) = cols.iter().position(|c| c.id == column_id) else {
            return Err(DomainError::NotFound {
                entity: "kanban_column".into(),
                id: column_id.to_string(),
            }
            .into());
        };
        let item = cols.remove(pos);
        let idx = target_index.min(cols.len());
        cols.insert(idx, item);
        let ids: Vec<Uuid> = cols.iter().map(|c| c.id).collect();
        self.db.reorder_columns(board_id, &ids, source)?;
        self.log_activity(
            source,
            audit,
            Some(board.profile_id),
            "kanban_column",
            Some(column_id),
            "reorder",
            "Изменён порядок колонок Kanban".into(),
            None,
        );
        self.sync.notify_local_change(source, "kanban_column");
        Ok(self.db.list_columns(board_id, false)?)
    }

    pub fn delete_column(
        &self,
        column_id: Uuid,
        move_tasks_to: Option<Uuid>,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<()> {
        let col = self
            .db
            .get_column(column_id)?
            .ok_or_else(|| DomainError::NotFound {
                entity: "kanban_column".into(),
                id: column_id.to_string(),
            })?;
        let board = self.get_board(col.board_id)?;
        self.db
            .delete_column_explicit(column_id, move_tasks_to, source)?;
        self.invalidate_stats_cache();
        self.log_activity(
            source,
            audit,
            Some(board.profile_id),
            "kanban_column",
            Some(column_id),
            "delete",
            format!("Удалена колонка «{}»", col.name),
            None,
        );
        self.sync.notify_local_change(source, "kanban_column");
        Ok(())
    }

    // ========================================================================
    // Tasks
    // ========================================================================

    fn resolve_board_and_column(
        &self,
        profile_id: Uuid,
        board_id: Option<Uuid>,
        column_id: Option<Uuid>,
    ) -> AppResult<(Board, KanbanColumn)> {
        let board = match board_id {
            Some(bid) => {
                let b = self.get_board(bid)?;
                if b.profile_id != profile_id {
                    return Err(DomainError::ProfileIsolation(
                        "board does not belong to target profile".into(),
                    )
                    .into());
                }
                b
            }
            None => self
                .db
                .list_boards(profile_id, false)?
                .into_iter()
                .next()
                .ok_or_else(|| {
                    DomainError::InvalidState("profile has no active Kanban board".into())
                })?,
        };
        let col = match column_id {
            Some(cid) => {
                let c = self
                    .db
                    .get_column(cid)?
                    .ok_or_else(|| DomainError::NotFound {
                        entity: "kanban_column".into(),
                        id: cid.to_string(),
                    })?;
                if c.board_id != board.id {
                    return Err(DomainError::validation(
                        "column_id",
                        "column does not belong to the selected board",
                    )
                    .into());
                }
                c
            }
            None => self
                .db
                .list_columns(board.id, false)?
                .into_iter()
                .next()
                .ok_or_else(|| DomainError::InvalidState("board has no active columns".into()))?,
        };
        Ok((board, col))
    }

    pub fn create_task(
        &self,
        input: CreateTaskInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Task> {
        let created = self.create_tasks_batch(vec![input], source, audit)?;
        Ok(created.into_iter().next().expect("non-empty batch"))
    }

    pub fn create_tasks_batch(
        &self,
        inputs: Vec<CreateTaskInput>,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Vec<Task>> {
        validate_batch_size(inputs.len())?;
        let now = Utc::now();
        let mut built = Vec::with_capacity(inputs.len());
        let mut reminder_specs: Vec<(Uuid, DateTime<Utc>, Vec<ReminderOffset>)> = Vec::new();

        for input in inputs {
            let (board, col) =
                self.resolve_board_and_column(input.profile_id, input.board_id, input.column_id)?;
            if let Some(sid) = input.subject_id {
                let subj = self.get_subject(sid)?;
                if subj.profile_id != input.profile_id {
                    return Err(DomainError::ProfileIsolation(
                        "subject does not belong to target profile".into(),
                    )
                    .into());
                }
            }
            let title = validate_non_empty("title", &input.title, MAX_TITLE_LEN)?;
            let description = validate_optional_text(
                "description",
                input.description.as_deref().unwrap_or(""),
                MAX_TEXT_LEN,
            )?;
            let notes = validate_optional_text(
                "notes",
                input.notes.as_deref().unwrap_or(""),
                MAX_TEXT_LEN,
            )?;
            let color = match input.color {
                Some(ref c) if !c.trim().is_empty() => Some(validate_hex_color("color", c)?),
                _ => None,
            };

            let mut tags = Vec::new();
            for tn in input.tag_names {
                if !tn.trim().is_empty() {
                    tags.push(self.get_or_create_tag(input.profile_id, &tn, None, source)?);
                }
            }
            let mut badges = Vec::new();
            for bn in input.badge_names {
                if !bn.trim().is_empty() {
                    badges.push(self.get_or_create_badge(input.profile_id, &bn, None, source)?);
                }
            }

            let task_id = Uuid::new_v4();
            let existing =
                self.db
                    .list_tasks(input.profile_id, Some(board.id), Some(col.id), None, false)?;
            let next_order =
                existing.iter().map(|t| t.sort_order).max().unwrap_or(-1) + 1 + built.len() as i32;

            let checklist: Vec<ChecklistItem> = input
                .checklist_titles
                .into_iter()
                .filter(|t| !t.trim().is_empty())
                .enumerate()
                .map(|(i, t)| ChecklistItem {
                    id: Uuid::new_v4(),
                    task_id,
                    title: t.trim().to_string(),
                    is_completed: false,
                    sort_order: i as i32,
                    created_at: now,
                    updated_at: now,
                })
                .collect();

            if let Some(due) = input.due_at
                && !input.reminder_offsets.is_empty()
            {
                reminder_specs.push((task_id, due, input.reminder_offsets));
            }

            built.push(Task {
                id: task_id,
                profile_id: input.profile_id,
                board_id: board.id,
                column_id: col.id,
                subject_id: input.subject_id,
                title,
                description,
                priority: input.priority.unwrap_or(TaskPriority::None),
                color,
                start_date: input.start_date,
                due_at: input.due_at,
                completed_at: if col.is_completed_column {
                    Some(now)
                } else {
                    None
                },
                notes,
                sort_order: next_order,
                is_archived: false,
                created_at: now,
                updated_at: now,
                tags,
                badges,
                checklist,
                linked_grade_ids: Vec::new(),
            });
        }

        self.db.insert_tasks_batch(&built, source)?;
        for (tid, due, offsets) in reminder_specs {
            for off in offsets {
                let remind_at = due - Duration::minutes(off.minutes());
                let rem = Reminder {
                    id: Uuid::new_v4(),
                    task_id: tid,
                    remind_at,
                    offset_minutes: off.minutes(),
                    is_fired: false,
                    created_at: now,
                };
                self.db.insert_reminder(&rem)?;
            }
        }
        self.invalidate_stats_cache();

        if built.len() == 1 {
            let t = &built[0];
            let undo_cmd = UndoCommand::TaskCreated { task: t.clone() };
            if source == RecordSource::Gui {
                self.undo.push(undo_cmd.clone());
            }
            self.log_activity(
                source,
                audit,
                Some(t.profile_id),
                "task",
                Some(t.id),
                "create",
                format!("Создана задача «{}»", t.title),
                Some(serde_json::to_value(&undo_cmd)?),
            );
        } else {
            let pid = built.first().map(|t| t.profile_id);
            self.log_activity(
                source,
                audit,
                pid,
                "task",
                None,
                "batch_create",
                format!("Создано задач пакетом: {}", built.len()),
                None,
            );
        }
        self.sync.notify_local_change(source, "task");
        Ok(built)
    }

    pub fn get_task(&self, id: Uuid) -> AppResult<Task> {
        self.db.get_task(id)?.ok_or_else(|| {
            DomainError::NotFound {
                entity: "task".into(),
                id: id.to_string(),
            }
            .into()
        })
    }

    pub fn list_tasks(
        &self,
        profile_id: Uuid,
        board_id: Option<Uuid>,
        column_id: Option<Uuid>,
        subject_id: Option<Uuid>,
        include_archived: bool,
    ) -> AppResult<Vec<Task>> {
        Ok(self.db.list_tasks(
            profile_id,
            board_id,
            column_id,
            subject_id,
            include_archived,
        )?)
    }

    pub fn update_task(
        &self,
        task_id: Uuid,
        input: UpdateTaskInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Task> {
        let before = self.get_task(task_id)?;
        let mut after = before.clone();
        if let Some(cid) = input.column_id {
            let col = self
                .db
                .get_column(cid)?
                .ok_or_else(|| DomainError::NotFound {
                    entity: "kanban_column".into(),
                    id: cid.to_string(),
                })?;
            after.column_id = col.id;
            after.board_id = col.board_id;
            if col.is_completed_column && after.completed_at.is_none() {
                after.completed_at = Some(Utc::now());
            } else if !col.is_completed_column && after.completed_at.is_some() {
                after.completed_at = None;
            }
        }
        if let Some(sid) = input.subject_id {
            if let Some(id) = sid {
                let subj = self.get_subject(id)?;
                if subj.profile_id != after.profile_id {
                    return Err(DomainError::ProfileIsolation(
                        "subject does not belong to task profile".into(),
                    )
                    .into());
                }
            }
            after.subject_id = sid;
        }
        if let Some(title) = input.title {
            after.title = validate_non_empty("title", &title, MAX_TITLE_LEN)?;
        }
        if let Some(desc) = input.description {
            after.description = validate_optional_text("description", &desc, MAX_TEXT_LEN)?;
        }
        if let Some(prio) = input.priority {
            after.priority = prio;
        }
        if let Some(color) = input.color {
            after.color = match color {
                Some(ref c) if !c.trim().is_empty() => Some(validate_hex_color("color", c)?),
                _ => None,
            };
        }
        if let Some(sd) = input.start_date {
            after.start_date = sd;
        }
        if let Some(due) = input.due_at {
            after.due_at = due;
        }
        if let Some(notes) = input.notes {
            after.notes = validate_optional_text("notes", &notes, MAX_TEXT_LEN)?;
        }
        if let Some(ord) = input.sort_order {
            after.sort_order = ord;
        }
        if let Some(arch) = input.is_archived {
            after.is_archived = arch;
        }
        after.updated_at = Utc::now();
        self.db.update_task(&after, source)?;
        self.invalidate_stats_cache();
        let undo_cmd = UndoCommand::TaskUpdated {
            before,
            after: after.clone(),
        };
        if source == RecordSource::Gui {
            self.undo.push(undo_cmd.clone());
        }
        self.log_activity(
            source,
            audit,
            Some(after.profile_id),
            "task",
            Some(after.id),
            "update",
            format!("Обновлена задача «{}»", after.title),
            Some(serde_json::to_value(&undo_cmd)?),
        );
        self.sync.notify_local_change(source, "task");
        Ok(after)
    }

    pub fn move_task(
        &self,
        task_id: Uuid,
        target_column_id: Uuid,
        target_order: Option<i32>,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Task> {
        let mut col_tasks = self.db.list_tasks(
            self.get_task(task_id)?.profile_id,
            None,
            Some(target_column_id),
            None,
            false,
        )?;
        col_tasks.retain(|t| t.id != task_id);
        let order = target_order
            .unwrap_or_else(|| col_tasks.iter().map(|t| t.sort_order).max().unwrap_or(-1) + 1);
        let moved = self.update_task(
            task_id,
            UpdateTaskInput {
                column_id: Some(target_column_id),
                sort_order: Some(order),
                ..Default::default()
            },
            source,
            audit,
        )?;
        Ok(moved)
    }

    pub fn complete_task(
        &self,
        task_id: Uuid,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Task> {
        let task = self.get_task(task_id)?;
        let cols = self.db.list_columns(task.board_id, false)?;
        let completed_col = cols.iter().find(|c| c.is_completed_column).map(|c| c.id);
        let mut updated = task.clone();
        if let Some(cid) = completed_col {
            updated.column_id = cid;
        }
        updated.completed_at = Some(Utc::now());
        updated.updated_at = Utc::now();
        self.db.update_task(&updated, source)?;
        self.invalidate_stats_cache();
        let undo_cmd = UndoCommand::TaskUpdated {
            before: task,
            after: updated.clone(),
        };
        if source == RecordSource::Gui {
            self.undo.push(undo_cmd.clone());
        }
        self.log_activity(
            source,
            audit,
            Some(updated.profile_id),
            "task",
            Some(updated.id),
            "complete",
            format!("Выполнена задача «{}»", updated.title),
            Some(serde_json::to_value(&undo_cmd)?),
        );
        self.sync.notify_local_change(source, "task");
        Ok(updated)
    }

    pub fn reopen_task(
        &self,
        task_id: Uuid,
        target_column_id: Option<Uuid>,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Task> {
        let task = self.get_task(task_id)?;
        let cols = self.db.list_columns(task.board_id, false)?;
        let open_col =
            target_column_id.or_else(|| cols.iter().find(|c| !c.is_completed_column).map(|c| c.id));
        let mut updated = task.clone();
        if let Some(cid) = open_col {
            updated.column_id = cid;
        }
        updated.completed_at = None;
        updated.updated_at = Utc::now();
        self.db.update_task(&updated, source)?;
        self.invalidate_stats_cache();
        self.log_activity(
            source,
            audit,
            Some(updated.profile_id),
            "task",
            Some(updated.id),
            "reopen",
            format!("Переоткрыта задача «{}»", updated.title),
            None,
        );
        self.sync.notify_local_change(source, "task");
        Ok(updated)
    }

    pub fn duplicate_task(&self, task_id: Uuid, source: RecordSource) -> AppResult<Task> {
        let orig = self.get_task(task_id)?;
        self.create_task(
            CreateTaskInput {
                profile_id: orig.profile_id,
                board_id: Some(orig.board_id),
                column_id: Some(orig.column_id),
                subject_id: orig.subject_id,
                title: format!("{} (копия)", orig.title),
                description: Some(orig.description),
                priority: Some(orig.priority),
                color: orig.color,
                start_date: orig.start_date,
                due_at: orig.due_at,
                notes: Some(orig.notes),
                tag_names: orig.tags.into_iter().map(|t| t.name).collect(),
                badge_names: orig.badges.into_iter().map(|b| b.name).collect(),
                checklist_titles: orig.checklist.into_iter().map(|c| c.title).collect(),
                reminder_offsets: vec![],
            },
            source,
            None,
        )
    }

    pub fn delete_task(
        &self,
        task_id: Uuid,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Task> {
        let existing = self.get_task(task_id)?;
        self.db.delete_task(task_id, source)?;
        self.invalidate_stats_cache();
        let undo_cmd = UndoCommand::TaskDeleted {
            task: existing.clone(),
        };
        if source == RecordSource::Gui {
            self.undo.push(undo_cmd.clone());
        }
        self.log_activity(
            source,
            audit,
            Some(existing.profile_id),
            "task",
            Some(existing.id),
            "delete",
            format!("Удалена задача «{}»", existing.title),
            Some(serde_json::to_value(&undo_cmd)?),
        );
        self.sync.notify_local_change(source, "task");
        Ok(existing)
    }

    pub fn get_overdue_tasks(&self, profile_id: Uuid) -> AppResult<Vec<Task>> {
        let now = Utc::now();
        let tasks = self.db.list_tasks(profile_id, None, None, None, false)?;
        Ok(tasks.into_iter().filter(|t| t.is_overdue(now)).collect())
    }

    pub fn get_upcoming_tasks(&self, profile_id: Uuid, within_days: i64) -> AppResult<Vec<Task>> {
        let now = Utc::now();
        let horizon = now + Duration::days(within_days.max(1));
        let tasks = self.db.list_tasks(profile_id, None, None, None, false)?;
        let mut out: Vec<Task> = tasks
            .into_iter()
            .filter(|t| {
                t.completed_at.is_none() && t.due_at.is_some_and(|d| d >= now && d <= horizon)
            })
            .collect();
        out.sort_by_key(|t| t.due_at);
        Ok(out)
    }

    pub fn search_tasks(&self, profile_id: Uuid, query: &str) -> AppResult<Vec<Task>> {
        let needle = query.trim().to_lowercase();
        let tasks = self.db.list_tasks(profile_id, None, None, None, false)?;
        if needle.is_empty() {
            return Ok(tasks);
        }
        Ok(tasks
            .into_iter()
            .filter(|t| {
                t.title.to_lowercase().contains(&needle)
                    || t.description.to_lowercase().contains(&needle)
                    || t.notes.to_lowercase().contains(&needle)
                    || t.tags
                        .iter()
                        .any(|tg| tg.name.to_lowercase().contains(&needle))
                    || t.badges
                        .iter()
                        .any(|b| b.name.to_lowercase().contains(&needle))
            })
            .collect())
    }

    // ========================================================================
    // Tags, Badges, Checklists & Grade Links
    // ========================================================================

    pub fn list_tags(&self, profile_id: Uuid) -> AppResult<Vec<Tag>> {
        Ok(self.db.list_tags(profile_id)?)
    }

    /// Deduplicates case-insensitively to prevent duplicate tags on repeated requests.
    pub fn get_or_create_tag(
        &self,
        profile_id: Uuid,
        name: &str,
        color: Option<&str>,
        source: RecordSource,
    ) -> AppResult<Tag> {
        let clean = validate_non_empty("name", name, 64)?;
        if let Some(existing) = self.db.find_tag_by_name(profile_id, &clean)? {
            return Ok(existing);
        }
        let tag = Tag {
            id: Uuid::new_v4(),
            profile_id,
            name: clean,
            color: validate_hex_color("color", color.unwrap_or("#445E91"))?,
            icon: None,
            description: String::new(),
        };
        self.db.insert_tag(&tag, source)?;
        self.sync.notify_local_change(source, "tag");
        Ok(tag)
    }

    pub fn update_tag(
        &self,
        profile_id: Uuid,
        tag_id: Uuid,
        name: Option<String>,
        color: Option<String>,
        description: Option<String>,
        source: RecordSource,
    ) -> AppResult<Tag> {
        let tags = self.db.list_tags(profile_id)?;
        let mut tag =
            tags.into_iter()
                .find(|t| t.id == tag_id)
                .ok_or_else(|| DomainError::NotFound {
                    entity: "tag".into(),
                    id: tag_id.to_string(),
                })?;
        if let Some(n) = name {
            tag.name = validate_non_empty("name", &n, 64)?;
        }
        if let Some(c) = color {
            tag.color = validate_hex_color("color", &c)?;
        }
        if let Some(d) = description {
            tag.description = validate_optional_text("description", &d, MAX_TITLE_LEN)?;
        }
        self.db.update_tag(&tag, source)?;
        self.sync.notify_local_change(source, "tag");
        Ok(tag)
    }

    pub fn assign_tag(&self, task_id: Uuid, tag_id: Uuid, source: RecordSource) -> AppResult<()> {
        self.db.assign_tag(task_id, tag_id, source)?;
        self.sync.notify_local_change(source, "task_tag");
        Ok(())
    }

    pub fn remove_tag(&self, task_id: Uuid, tag_id: Uuid, source: RecordSource) -> AppResult<()> {
        self.db.remove_tag(task_id, tag_id, source)?;
        self.sync.notify_local_change(source, "task_tag");
        Ok(())
    }

    pub fn list_badges(&self, profile_id: Uuid) -> AppResult<Vec<Badge>> {
        Ok(self.db.list_badges(profile_id)?)
    }

    pub fn get_or_create_badge(
        &self,
        profile_id: Uuid,
        name: &str,
        color: Option<&str>,
        source: RecordSource,
    ) -> AppResult<Badge> {
        let clean = validate_non_empty("name", name, 64)?;
        if let Some(existing) = self.db.find_badge_by_name(profile_id, &clean)? {
            return Ok(existing);
        }
        let badge = Badge {
            id: Uuid::new_v4(),
            profile_id,
            name: clean,
            color: validate_hex_color("color", color.unwrap_or("#7C3AED"))?,
            icon: None,
            description: String::new(),
        };
        self.db.insert_badge(&badge, source)?;
        self.sync.notify_local_change(source, "badge");
        Ok(badge)
    }

    pub fn assign_badge(
        &self,
        task_id: Uuid,
        badge_id: Uuid,
        source: RecordSource,
    ) -> AppResult<()> {
        self.db.assign_badge(task_id, badge_id, source)?;
        self.sync.notify_local_change(source, "task_badge");
        Ok(())
    }

    pub fn remove_badge(
        &self,
        task_id: Uuid,
        badge_id: Uuid,
        source: RecordSource,
    ) -> AppResult<()> {
        self.db.remove_badge(task_id, badge_id, source)?;
        self.sync.notify_local_change(source, "task_badge");
        Ok(())
    }

    pub fn add_checklist_item(
        &self,
        task_id: Uuid,
        title: String,
        source: RecordSource,
    ) -> AppResult<ChecklistItem> {
        let task = self.get_task(task_id)?;
        let title = validate_non_empty("title", &title, MAX_TITLE_LEN)?;
        let next_order = task
            .checklist
            .iter()
            .map(|c| c.sort_order)
            .max()
            .unwrap_or(-1)
            + 1;
        let now = Utc::now();
        let item = ChecklistItem {
            id: Uuid::new_v4(),
            task_id,
            title,
            is_completed: false,
            sort_order: next_order,
            created_at: now,
            updated_at: now,
        };
        self.db.insert_checklist_item(&item, source)?;
        self.sync.notify_local_change(source, "checklist");
        Ok(item)
    }

    pub fn update_checklist_item(
        &self,
        item_id: Uuid,
        title: Option<String>,
        is_completed: Option<bool>,
        sort_order: Option<i32>,
        source: RecordSource,
    ) -> AppResult<ChecklistItem> {
        let mut item =
            self.db
                .get_checklist_item(item_id)?
                .ok_or_else(|| DomainError::NotFound {
                    entity: "checklist_item".into(),
                    id: item_id.to_string(),
                })?;
        if let Some(t) = title {
            item.title = validate_non_empty("title", &t, MAX_TITLE_LEN)?;
        }
        if let Some(done) = is_completed {
            item.is_completed = done;
        }
        if let Some(ord) = sort_order {
            item.sort_order = ord;
        }
        item.updated_at = Utc::now();
        self.db.update_checklist_item(&item, source)?;
        self.sync.notify_local_change(source, "checklist");
        Ok(item)
    }

    pub fn delete_checklist_item(&self, item_id: Uuid, source: RecordSource) -> AppResult<()> {
        self.db.delete_checklist_item(item_id, source)?;
        self.sync.notify_local_change(source, "checklist");
        Ok(())
    }

    pub fn link_task_grade(
        &self,
        task_id: Uuid,
        grade_id: Uuid,
        source: RecordSource,
    ) -> AppResult<()> {
        let t = self.get_task(task_id)?;
        let g = self.get_grade(grade_id)?;
        if t.profile_id != g.profile_id {
            return Err(DomainError::ProfileIsolation(
                "cannot link task and grade from different profiles".into(),
            )
            .into());
        }
        self.db.link_task_grade(task_id, grade_id, source)?;
        self.sync.notify_local_change(source, "task_grade_link");
        Ok(())
    }

    pub fn unlink_task_grade(
        &self,
        task_id: Uuid,
        grade_id: Uuid,
        source: RecordSource,
    ) -> AppResult<()> {
        self.db.unlink_task_grade(task_id, grade_id, source)?;
        self.sync.notify_local_change(source, "task_grade_link");
        Ok(())
    }

    pub fn set_task_reminders(
        &self,
        task_id: Uuid,
        offsets: &[ReminderOffset],
    ) -> AppResult<Vec<Reminder>> {
        let task = self.get_task(task_id)?;
        self.db.delete_task_reminders(task_id)?;
        let Some(due) = task.due_at else {
            return Ok(Vec::new());
        };
        let now = Utc::now();
        let mut created = Vec::new();
        for off in offsets {
            let rem = Reminder {
                id: Uuid::new_v4(),
                task_id,
                remind_at: due - Duration::minutes(off.minutes()),
                offset_minutes: off.minutes(),
                is_fired: false,
                created_at: now,
            };
            self.db.insert_reminder(&rem)?;
            created.push(rem);
        }
        Ok(created)
    }
}
