use chrono::{NaiveDate, Utc};
use journal_domain::{
    AcademicPeriod, Board, DomainError, Holiday, KanbanColumn, MarkCategory, MarkType, PeriodType,
    Profile, RecordSource, Subject, validate_date_range, validate_hex_color, validate_non_empty,
    validate_optional_text, validate_target_average, MAX_SHORT_NAME_LEN, MAX_TEXT_LEN,
    MAX_TITLE_LEN,
};
use journal_storage::DeleteImpact;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppResult;
use crate::i18n;
use crate::services::AppServices;
use crate::services::ops::AuditMeta;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateProfileInput {
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub institution: Option<String>,
    pub group_name: Option<String>,
    pub academic_year: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateProfileInput {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub institution: Option<String>,
    pub group_name: Option<String>,
    pub academic_year: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateSubjectInput {
    pub profile_id: Uuid,
    pub name: String,
    pub short_name: Option<String>,
    pub teacher: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub academic_period_id: Option<Uuid>,
    pub target_average: Option<Decimal>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateSubjectInput {
    pub name: Option<String>,
    pub short_name: Option<String>,
    pub teacher: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub academic_period_id: Option<Option<Uuid>>,
    pub target_average: Option<Option<Decimal>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubjectLookupResult {
    pub exact: Option<Subject>,
    pub candidates: Vec<Subject>,
    pub ambiguous: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAcademicPeriodInput {
    pub profile_id: Uuid,
    pub name: String,
    pub period_type: PeriodType,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMarkTypeInput {
    pub profile_id: Uuid,
    pub code: String,
    pub name: String,
    pub short_label: String,
    pub category: MarkCategory,
    pub color: String,
    pub icon: Option<String>,
    pub counts_in_average: Option<bool>,
    pub numeric_value: Option<Decimal>,
}

impl AppServices {
    // ========================================================================
    // Profiles
    // ========================================================================

    pub fn list_profiles(&self, include_archived: bool) -> AppResult<Vec<Profile>> {
        Ok(self.db.list_profiles(include_archived)?)
    }

    pub fn get_profile(&self, id: Uuid) -> AppResult<Profile> {
        self.db
            .get_profile(id)?
            .ok_or_else(|| DomainError::NotFound {
                entity: "profile".into(),
                id: id.to_string(),
            }
            .into())
    }

    pub fn create_profile(
        &self,
        input: CreateProfileInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Profile> {
        let name = validate_non_empty("name", &input.name, MAX_TITLE_LEN)?;
        let color = validate_hex_color("color", input.color.as_deref().unwrap_or("#445E91"))?;
        let now = Utc::now();
        let profile = Profile {
            id: Uuid::new_v4(),
            name,
            icon: input.icon.unwrap_or_else(|| "school".into()),
            color,
            institution: validate_optional_text(
                "institution",
                input.institution.as_deref().unwrap_or(""),
                MAX_TITLE_LEN,
            )?,
            group_name: validate_optional_text(
                "group_name",
                input.group_name.as_deref().unwrap_or(""),
                MAX_TITLE_LEN,
            )?,
            academic_year: validate_optional_text(
                "academic_year",
                input.academic_year.as_deref().unwrap_or(""),
                64,
            )?,
            description: validate_optional_text(
                "description",
                input.description.as_deref().unwrap_or(""),
                MAX_TEXT_LEN,
            )?,
            is_archived: false,
            created_at: now,
            updated_at: now,
        };
        self.db.insert_profile(&profile, source)?;

        // Create default Kanban board with 4 standard columns for this profile
        let lang = self.db.load_settings().map(|s| s.language).unwrap_or_default();
        let board_name = match lang {
            journal_domain::Language::Ru => "Основная доска",
            journal_domain::Language::En => "Main Board",
        };
        let board = Board {
            id: Uuid::new_v4(),
            profile_id: profile.id,
            name: board_name.into(),
            description: String::new(),
            color: profile.color.clone(),
            sort_order: 0,
            is_archived: false,
            created_at: now,
            updated_at: now,
        };
        self.db.insert_board(&board, source)?;
        for (idx, (col_name, col_color, is_done)) in i18n::default_column_names(lang).into_iter().enumerate() {
            let col = KanbanColumn {
                id: Uuid::new_v4(),
                board_id: board.id,
                name: col_name.into(),
                color: col_color.into(),
                sort_order: idx as i32,
                is_collapsed: false,
                is_archived: false,
                wip_limit: None,
                is_completed_column: is_done,
                created_at: now,
                updated_at: now,
            };
            self.db.insert_column(&col, source)?;
        }

        self.log_activity(
            source,
            audit,
            Some(profile.id),
            "profile",
            Some(profile.id),
            "create",
            format!("Создан профиль «{}»", profile.name),
            None,
        );
        self.sync.notify_local_change(source, "profile");
        Ok(profile)
    }

    pub fn update_profile(
        &self,
        id: Uuid,
        input: UpdateProfileInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Profile> {
        let mut profile = self.get_profile(id)?;
        if let Some(name) = input.name {
            profile.name = validate_non_empty("name", &name, MAX_TITLE_LEN)?;
        }
        if let Some(icon) = input.icon {
            profile.icon = validate_non_empty("icon", &icon, 64)?;
        }
        if let Some(color) = input.color {
            profile.color = validate_hex_color("color", &color)?;
        }
        if let Some(inst) = input.institution {
            profile.institution = validate_optional_text("institution", &inst, MAX_TITLE_LEN)?;
        }
        if let Some(grp) = input.group_name {
            profile.group_name = validate_optional_text("group_name", &grp, MAX_TITLE_LEN)?;
        }
        if let Some(year) = input.academic_year {
            profile.academic_year = validate_optional_text("academic_year", &year, 64)?;
        }
        if let Some(desc) = input.description {
            profile.description = validate_optional_text("description", &desc, MAX_TEXT_LEN)?;
        }
        profile.updated_at = Utc::now();
        self.db.update_profile(&profile, source)?;
        self.log_activity(
            source,
            audit,
            Some(profile.id),
            "profile",
            Some(profile.id),
            "update",
            format!("Обновлён профиль «{}»", profile.name),
            None,
        );
        self.sync.notify_local_change(source, "profile");
        Ok(profile)
    }

    pub fn duplicate_profile(
        &self,
        id: Uuid,
        new_name: Option<String>,
        copy_subjects: bool,
        source: RecordSource,
    ) -> AppResult<Profile> {
        let orig = self.get_profile(id)?;
        let name = new_name.unwrap_or_else(|| format!("{} (копия)", orig.name));
        let created = self.create_profile(
            CreateProfileInput {
                name,
                icon: Some(orig.icon),
                color: Some(orig.color),
                institution: Some(orig.institution),
                group_name: Some(orig.group_name),
                academic_year: Some(orig.academic_year),
                description: Some(orig.description),
            },
            source,
            None,
        )?;
        if copy_subjects {
            for subj in self.db.list_subjects(id, false)? {
                let _ = self.create_subject(
                    CreateSubjectInput {
                        profile_id: created.id,
                        name: subj.name,
                        short_name: Some(subj.short_name),
                        teacher: Some(subj.teacher),
                        color: Some(subj.color),
                        icon: Some(subj.icon),
                        description: Some(subj.description),
                        academic_period_id: None,
                        target_average: subj.target_average,
                    },
                    source,
                    None,
                );
            }
        }
        Ok(created)
    }

    pub fn set_profile_archived(
        &self,
        id: Uuid,
        archived: bool,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Profile> {
        let mut p = self.get_profile(id)?;
        if archived {
            let active = self.db.list_profiles(false)?;
            if active.len() <= 1 && active.iter().any(|x| x.id == id) {
                return Err(DomainError::InvalidState(
                    "cannot archive the last remaining active profile".into(),
                )
                .into());
            }
        }
        p.is_archived = archived;
        p.updated_at = Utc::now();
        self.db.update_profile(&p, source)?;
        self.log_activity(
            source,
            audit,
            Some(p.id),
            "profile",
            Some(p.id),
            if archived { "archive" } else { "restore" },
            format!(
                "{} профиль «{}»",
                if archived { "Архивирован" } else { "Восстановлен" },
                p.name
            ),
            None,
        );
        self.sync.notify_local_change(source, "profile");
        Ok(p)
    }

    pub fn profile_delete_impact(&self, id: Uuid) -> AppResult<DeleteImpact> {
        Ok(self.db.profile_delete_impact(id)?)
    }

    pub fn delete_profile(
        &self,
        id: Uuid,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<()> {
        let p = self.get_profile(id)?;
        let active = self.db.list_profiles(false)?;
        if active.len() <= 1 && active.iter().any(|x| x.id == id) {
            return Err(DomainError::InvalidState(
                "cannot delete the only active profile".into(),
            )
            .into());
        }
        self.db.delete_profile_cascade_explicit(id, source)?;
        // Switch active profile if we deleted it
        let settings = self.db.load_settings()?;
        if settings.active_profile_id == Some(id)
            && let Some(next) = self.db.list_profiles(false)?.into_iter().next()
        {
            let _ = self.set_active_profile(next.id);
        }
        self.invalidate_stats_cache();
        self.log_activity(
            source,
            audit,
            None,
            "profile",
            Some(id),
            "delete",
            format!("Удалён профиль «{}»", p.name),
            None,
        );
        self.sync.notify_local_change(source, "profile");
        Ok(())
    }

    // ========================================================================
    // Subjects
    // ========================================================================

    pub fn list_subjects(
        &self,
        profile_id: Uuid,
        include_archived: bool,
    ) -> AppResult<Vec<Subject>> {
        Ok(self.db.list_subjects(profile_id, include_archived)?)
    }

    pub fn get_subject(&self, id: Uuid) -> AppResult<Subject> {
        self.db
            .get_subject(id)?
            .ok_or_else(|| DomainError::NotFound {
                entity: "subject".into(),
                id: id.to_string(),
            }
            .into())
    }

    /// Searches subjects by name or short_name within a profile.
    /// Returns exact match when unambiguous, or all matching candidates when ambiguous (never guesses randomly).
    pub fn find_subject(&self, profile_id: Uuid, query: &str) -> AppResult<SubjectLookupResult> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(SubjectLookupResult {
                exact: None,
                candidates: vec![],
                ambiguous: false,
            });
        }
        let subjects = self.db.list_subjects(profile_id, false)?;
        // 1. Exact match on name or short_name
        let exact_matches: Vec<Subject> = subjects
            .iter()
            .filter(|s| {
                s.name.trim().to_lowercase() == needle
                    || (!s.short_name.is_empty() && s.short_name.trim().to_lowercase() == needle)
            })
            .cloned()
            .collect();
        if exact_matches.len() == 1 {
            return Ok(SubjectLookupResult {
                exact: Some(exact_matches[0].clone()),
                candidates: exact_matches,
                ambiguous: false,
            });
        }
        if exact_matches.len() > 1 {
            return Ok(SubjectLookupResult {
                exact: None,
                candidates: exact_matches,
                ambiguous: true,
            });
        }

        // 2. Partial substring match
        let partial: Vec<Subject> = subjects
            .into_iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&needle)
                    || s.short_name.to_lowercase().contains(&needle)
            })
            .collect();
        match partial.len() {
            0 => Ok(SubjectLookupResult {
                exact: None,
                candidates: vec![],
                ambiguous: false,
            }),
            1 => Ok(SubjectLookupResult {
                exact: Some(partial[0].clone()),
                candidates: partial,
                ambiguous: false,
            }),
            _ => Ok(SubjectLookupResult {
                exact: None,
                candidates: partial,
                ambiguous: true,
            }),
        }
    }

    pub fn create_subject(
        &self,
        input: CreateSubjectInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Subject> {
        let _ = self.get_profile(input.profile_id)?;
        let name = validate_non_empty("name", &input.name, MAX_TITLE_LEN)?;
        let short_name = match input.short_name {
            Some(s) if !s.trim().is_empty() => validate_non_empty("short_name", &s, MAX_SHORT_NAME_LEN)?,
            _ => name.chars().take(6).collect(),
        };
        let color = validate_hex_color("color", input.color.as_deref().unwrap_or("#445E91"))?;
        let target_average = match input.target_average {
            Some(t) => Some(validate_target_average(t)?),
            None => None,
        };
        let existing = self.db.list_subjects(input.profile_id, true)?;
        let next_order = existing.iter().map(|s| s.sort_order).max().unwrap_or(-1) + 1;
        let now = Utc::now();
        let subject = Subject {
            id: Uuid::new_v4(),
            profile_id: input.profile_id,
            academic_period_id: input.academic_period_id,
            name,
            short_name,
            teacher: validate_optional_text(
                "teacher",
                input.teacher.as_deref().unwrap_or(""),
                MAX_TITLE_LEN,
            )?,
            color,
            icon: input.icon.unwrap_or_else(|| "book".into()),
            description: validate_optional_text(
                "description",
                input.description.as_deref().unwrap_or(""),
                MAX_TEXT_LEN,
            )?,
            sort_order: next_order,
            is_archived: false,
            target_average,
            created_at: now,
            updated_at: now,
        };
        self.db.insert_subject(&subject, source)?;
        self.invalidate_stats_cache();
        self.log_activity(
            source,
            audit,
            Some(subject.profile_id),
            "subject",
            Some(subject.id),
            "create",
            format!("Создан предмет «{}»", subject.name),
            None,
        );
        self.sync.notify_local_change(source, "subject");
        Ok(subject)
    }

    pub fn update_subject(
        &self,
        id: Uuid,
        input: UpdateSubjectInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Subject> {
        let mut s = self.get_subject(id)?;
        if let Some(name) = input.name {
            s.name = validate_non_empty("name", &name, MAX_TITLE_LEN)?;
        }
        if let Some(short) = input.short_name {
            s.short_name = validate_optional_text("short_name", &short, MAX_SHORT_NAME_LEN)?;
        }
        if let Some(teacher) = input.teacher {
            s.teacher = validate_optional_text("teacher", &teacher, MAX_TITLE_LEN)?;
        }
        if let Some(color) = input.color {
            s.color = validate_hex_color("color", &color)?;
        }
        if let Some(icon) = input.icon {
            s.icon = validate_non_empty("icon", &icon, 64)?;
        }
        if let Some(desc) = input.description {
            s.description = validate_optional_text("description", &desc, MAX_TEXT_LEN)?;
        }
        if let Some(ap) = input.academic_period_id {
            s.academic_period_id = ap;
        }
        if let Some(tgt) = input.target_average {
            s.target_average = match tgt {
                Some(v) => Some(validate_target_average(v)?),
                None => None,
            };
        }
        s.updated_at = Utc::now();
        self.db.update_subject(&s, source)?;
        self.invalidate_stats_cache();
        self.log_activity(
            source,
            audit,
            Some(s.profile_id),
            "subject",
            Some(s.id),
            "update",
            format!("Обновлён предмет «{}»", s.name),
            None,
        );
        self.sync.notify_local_change(source, "subject");
        Ok(s)
    }

    pub fn set_subject_archived(
        &self,
        id: Uuid,
        archived: bool,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Subject> {
        let mut s = self.get_subject(id)?;
        s.is_archived = archived;
        s.updated_at = Utc::now();
        self.db.update_subject(&s, source)?;
        self.invalidate_stats_cache();
        self.log_activity(
            source,
            audit,
            Some(s.profile_id),
            "subject",
            Some(s.id),
            if archived { "archive" } else { "restore" },
            format!(
                "{} предмет «{}»",
                if archived { "Архивирован" } else { "Восстановлен" },
                s.name
            ),
            None,
        );
        self.sync.notify_local_change(source, "subject");
        Ok(s)
    }

    pub fn reorder_subjects(
        &self,
        profile_id: Uuid,
        ordered_ids: &[Uuid],
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<()> {
        self.db.reorder_subjects(profile_id, ordered_ids, source)?;
        self.log_activity(
            source,
            audit,
            Some(profile_id),
            "subject",
            None,
            "reorder",
            "Изменён порядок предметов".into(),
            None,
        );
        self.sync.notify_local_change(source, "subject");
        Ok(())
    }

    pub fn subject_delete_impact(&self, id: Uuid) -> AppResult<DeleteImpact> {
        Ok(self.db.subject_delete_impact(id)?)
    }

    pub fn delete_subject(
        &self,
        id: Uuid,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<()> {
        let s = self.get_subject(id)?;
        self.db.delete_subject_explicit(id, source)?;
        self.invalidate_stats_cache();
        self.log_activity(
            source,
            audit,
            Some(s.profile_id),
            "subject",
            Some(id),
            "delete",
            format!("Удалён предмет «{}»", s.name),
            None,
        );
        self.sync.notify_local_change(source, "subject");
        Ok(())
    }

    // ========================================================================
    // Academic Periods
    // ========================================================================

    pub fn list_academic_periods(
        &self,
        profile_id: Uuid,
        include_archived: bool,
    ) -> AppResult<Vec<AcademicPeriod>> {
        Ok(self.db.list_academic_periods(profile_id, include_archived)?)
    }

    pub fn create_academic_period(
        &self,
        input: CreateAcademicPeriodInput,
        source: RecordSource,
    ) -> AppResult<AcademicPeriod> {
        let _ = self.get_profile(input.profile_id)?;
        let name = validate_non_empty("name", &input.name, MAX_TITLE_LEN)?;
        validate_date_range(input.start_date, input.end_date)?;
        let now = Utc::now();
        let period = AcademicPeriod {
            id: Uuid::new_v4(),
            profile_id: input.profile_id,
            name,
            period_type: input.period_type,
            start_date: input.start_date,
            end_date: input.end_date,
            is_archived: false,
            created_at: now,
            updated_at: now,
        };
        self.db.insert_academic_period(&period, source)?;
        self.sync.notify_local_change(source, "academic_period");
        Ok(period)
    }

    pub fn archive_academic_period(
        &self,
        profile_id: Uuid,
        period_id: Uuid,
        archived: bool,
        source: RecordSource,
    ) -> AppResult<AcademicPeriod> {
        let periods = self.db.list_academic_periods(profile_id, true)?;
        let mut period = periods
            .into_iter()
            .find(|p| p.id == period_id)
            .ok_or_else(|| DomainError::NotFound {
                entity: "academic_period".into(),
                id: period_id.to_string(),
            })?;
        period.is_archived = archived;
        period.updated_at = Utc::now();
        self.db.update_academic_period(&period, source)?;
        self.sync.notify_local_change(source, "academic_period");
        Ok(period)
    }

    // ========================================================================
    // Mark Types & Holidays
    // ========================================================================

    pub fn list_mark_types(&self, profile_id: Option<Uuid>) -> AppResult<Vec<MarkType>> {
        Ok(self.db.list_mark_types(profile_id)?)
    }

    pub fn create_custom_mark_type(
        &self,
        input: CreateMarkTypeInput,
        source: RecordSource,
    ) -> AppResult<MarkType> {
        let _ = self.get_profile(input.profile_id)?;
        let code = validate_non_empty("code", &input.code, 32)?.to_lowercase();
        let name = validate_non_empty("name", &input.name, 64)?;
        let short_label = validate_non_empty("short_label", &input.short_label, 12)?;
        let color = validate_hex_color("color", &input.color)?;
        // By default custom marks do NOT affect GPA unless explicitly enabled with a numeric value
        let counts_in_average = input.counts_in_average.unwrap_or(false) && input.numeric_value.is_some();
        let existing = self.db.list_mark_types(Some(input.profile_id))?;
        let next_order = existing.iter().map(|m| m.sort_order).max().unwrap_or(10) + 1;

        let mt = MarkType {
            id: Uuid::new_v4(),
            profile_id: Some(input.profile_id),
            code,
            name,
            short_label,
            category: input.category,
            color,
            icon: input.icon,
            counts_in_average,
            numeric_value: input.numeric_value,
            is_builtin: false,
            sort_order: next_order,
        };
        self.db.insert_mark_type(&mt, source)?;
        self.sync.notify_local_change(source, "mark_type");
        Ok(mt)
    }

    pub fn delete_custom_mark_type(&self, id: Uuid, source: RecordSource) -> AppResult<()> {
        self.db.delete_custom_mark_type(id, source)?;
        self.sync.notify_local_change(source, "mark_type");
        Ok(())
    }

    pub fn list_holidays(&self, profile_id: Option<Uuid>) -> AppResult<Vec<Holiday>> {
        Ok(self.db.list_holidays(profile_id)?)
    }

    pub fn add_holiday(
        &self,
        profile_id: Option<Uuid>,
        date: NaiveDate,
        name: String,
        source: RecordSource,
    ) -> AppResult<Holiday> {
        let name = validate_non_empty("name", &name, MAX_TITLE_LEN)?;
        let h = Holiday {
            id: Uuid::new_v4(),
            profile_id,
            date,
            name,
        };
        self.db.insert_holiday(&h, source)?;
        self.sync.notify_local_change(source, "holiday");
        Ok(h)
    }

    pub fn delete_holiday(&self, id: Uuid, source: RecordSource) -> AppResult<()> {
        self.db.delete_holiday(id, source)?;
        self.sync.notify_local_change(source, "holiday");
        Ok(())
    }
}
