use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Profiles & Academic Periods
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub institution: String,
    pub group_name: String,
    pub academic_year: String,
    pub description: String,
    pub is_archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodType {
    AcademicYear,
    Semester,
    Quarter,
    Trimester,
    Custom,
}

impl PeriodType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AcademicYear => "academic_year",
            Self::Semester => "semester",
            Self::Quarter => "quarter",
            Self::Trimester => "trimester",
            Self::Custom => "custom",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "academic_year" | "year" => Some(Self::AcademicYear),
            "semester" => Some(Self::Semester),
            "quarter" => Some(Self::Quarter),
            "trimester" => Some(Self::Trimester),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcademicPeriod {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub name: String,
    pub period_type: PeriodType,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub is_archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// Subjects
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub academic_period_id: Option<Uuid>,
    pub name: String,
    pub short_name: String,
    pub teacher: String,
    pub color: String,
    pub icon: String,
    pub description: String,
    pub sort_order: i32,
    pub is_archived: bool,
    pub target_average: Option<Decimal>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// Mark Types, Grades & Attendance
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkCategory {
    NumericGrade,
    Attendance,
    PassFail,
    CustomStatus,
}

impl MarkCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NumericGrade => "numeric_grade",
            Self::Attendance => "attendance",
            Self::PassFail => "pass_fail",
            Self::CustomStatus => "custom_status",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "numeric_grade" => Some(Self::NumericGrade),
            "attendance" => Some(Self::Attendance),
            "pass_fail" => Some(Self::PassFail),
            "custom_status" => Some(Self::CustomStatus),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkType {
    pub id: Uuid,
    /// `None` for built-in mark types shared across all profiles; `Some(profile_id)` for custom types.
    pub profile_id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub short_label: String,
    pub category: MarkCategory,
    pub color: String,
    pub icon: Option<String>,
    pub counts_in_average: bool,
    pub numeric_value: Option<Decimal>,
    pub is_builtin: bool,
    pub sort_order: i32,
}

/// Deterministic UUIDs for built-in mark types so both SQLite and domain logic stay consistent.
pub mod builtin_marks {
    use super::{MarkCategory, MarkType};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    pub const GRADE_5_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0005);
    pub const GRADE_4_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0004);
    pub const GRADE_3_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0003);
    pub const GRADE_2_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0002);
    pub const GRADE_1_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0001);
    pub const ABSENT_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0010);
    pub const EXCUSED_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0011);
    pub const PASS_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0012);
    pub const FAIL_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0013);
    pub const PRESENT_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0014);

    pub fn all_builtins() -> Vec<MarkType> {
        vec![
            MarkType {
                id: GRADE_5_ID,
                profile_id: None,
                code: "grade_5".into(),
                name: "Отлично (5)".into(),
                short_label: "5".into(),
                category: MarkCategory::NumericGrade,
                color: "#2E7D32".into(),
                icon: None,
                counts_in_average: true,
                numeric_value: Some(Decimal::from(5)),
                is_builtin: true,
                sort_order: 1,
            },
            MarkType {
                id: GRADE_4_ID,
                profile_id: None,
                code: "grade_4".into(),
                name: "Хорошо (4)".into(),
                short_label: "4".into(),
                category: MarkCategory::NumericGrade,
                color: "#00897B".into(),
                icon: None,
                counts_in_average: true,
                numeric_value: Some(Decimal::from(4)),
                is_builtin: true,
                sort_order: 2,
            },
            MarkType {
                id: GRADE_3_ID,
                profile_id: None,
                code: "grade_3".into(),
                name: "Удовлетворительно (3)".into(),
                short_label: "3".into(),
                category: MarkCategory::NumericGrade,
                color: "#D97706".into(),
                icon: None,
                counts_in_average: true,
                numeric_value: Some(Decimal::from(3)),
                is_builtin: true,
                sort_order: 3,
            },
            MarkType {
                id: GRADE_2_ID,
                profile_id: None,
                code: "grade_2".into(),
                name: "Неудовлетворительно (2)".into(),
                short_label: "2".into(),
                category: MarkCategory::NumericGrade,
                color: "#EA580C".into(),
                icon: None,
                counts_in_average: true,
                numeric_value: Some(Decimal::from(2)),
                is_builtin: true,
                sort_order: 4,
            },
            MarkType {
                id: GRADE_1_ID,
                profile_id: None,
                code: "grade_1".into(),
                name: "Единица (1)".into(),
                short_label: "1".into(),
                category: MarkCategory::NumericGrade,
                color: "#DC2626".into(),
                icon: None,
                counts_in_average: true,
                numeric_value: Some(Decimal::from(1)),
                is_builtin: true,
                sort_order: 5,
            },
            MarkType {
                id: ABSENT_ID,
                profile_id: None,
                code: "absent".into(),
                name: "Отсутствие (Н/Б)".into(),
                short_label: "Н/Б".into(),
                category: MarkCategory::Attendance,
                color: "#E11D48".into(),
                icon: None,
                counts_in_average: false,
                numeric_value: None,
                is_builtin: true,
                sort_order: 6,
            },
            MarkType {
                id: EXCUSED_ID,
                profile_id: None,
                code: "excused".into(),
                name: "Уважительная причина".into(),
                short_label: "Уваж.".into(),
                category: MarkCategory::Attendance,
                color: "#2563EB".into(),
                icon: None,
                counts_in_average: false,
                numeric_value: None,
                is_builtin: true,
                sort_order: 7,
            },
            MarkType {
                id: PASS_ID,
                profile_id: None,
                code: "pass".into(),
                name: "Зачтено".into(),
                short_label: "Зач.".into(),
                category: MarkCategory::PassFail,
                color: "#0D9488".into(),
                icon: None,
                counts_in_average: false,
                numeric_value: None,
                is_builtin: true,
                sort_order: 8,
            },
            MarkType {
                id: FAIL_ID,
                profile_id: None,
                code: "fail".into(),
                name: "Не зачтено".into(),
                short_label: "Н/З".into(),
                category: MarkCategory::PassFail,
                color: "#B91C1C".into(),
                icon: None,
                counts_in_average: false,
                numeric_value: None,
                is_builtin: true,
                sort_order: 9,
            },
            MarkType {
                id: PRESENT_ID,
                profile_id: None,
                code: "present".into(),
                name: "Присутствие".into(),
                short_label: "Прис.".into(),
                category: MarkCategory::Attendance,
                color: "#475569".into(),
                icon: None,
                counts_in_average: false,
                numeric_value: None,
                is_builtin: true,
                sort_order: 10,
            },
        ]
    }

    pub fn grade_id_for_value(value: u8) -> Option<Uuid> {
        match value {
            1 => Some(GRADE_1_ID),
            2 => Some(GRADE_2_ID),
            3 => Some(GRADE_3_ID),
            4 => Some(GRADE_4_ID),
            5 => Some(GRADE_5_ID),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkType {
    #[default]
    ClassAnswer,
    Homework,
    ControlWork,
    IndependentWork,
    LabWork,
    PracticalWork,
    Test,
    Project,
    Exam,
    Credit,
    Other,
}

impl WorkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClassAnswer => "class_answer",
            Self::Homework => "homework",
            Self::ControlWork => "control_work",
            Self::IndependentWork => "independent_work",
            Self::LabWork => "lab_work",
            Self::PracticalWork => "practical_work",
            Self::Test => "test",
            Self::Project => "project",
            Self::Exam => "exam",
            Self::Credit => "credit",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "class_answer" | "answer" | "ответ" => Some(Self::ClassAnswer),
            "homework" | "дз" => Some(Self::Homework),
            "control_work" | "control" | "контрольная" => Some(Self::ControlWork),
            "independent_work" | "independent" | "самостоятельная" => {
                Some(Self::IndependentWork)
            }
            "lab_work" | "lab" | "лабораторная" => Some(Self::LabWork),
            "practical_work" | "practical" | "практическая" => {
                Some(Self::PracticalWork)
            }
            "test" | "тест" => Some(Self::Test),
            "project" | "проект" => Some(Self::Project),
            "exam" | "экзамен" => Some(Self::Exam),
            "credit" | "зачет" | "зачёт" => Some(Self::Credit),
            "other" | "другое" => Some(Self::Other),
            _ => None,
        }
    }

    pub fn default_weight(&self) -> Decimal {
        match self {
            Self::ControlWork => Decimal::from(2),
            Self::Exam => Decimal::from(3),
            _ => Decimal::from(1),
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::ClassAnswer,
            Self::Homework,
            Self::ControlWork,
            Self::IndependentWork,
            Self::LabWork,
            Self::PracticalWork,
            Self::Test,
            Self::Project,
            Self::Exam,
            Self::Credit,
            Self::Other,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordSource {
    #[default]
    Gui,
    Mcp,
    Import,
}

impl RecordSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Gui => "gui",
            Self::Mcp => "mcp",
            Self::Import => "import",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "gui" => Some(Self::Gui),
            "mcp" => Some(Self::Mcp),
            "import" => Some(Self::Import),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GradeEntry {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub subject_id: Uuid,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub mark_type_id: Uuid,
    /// Snapshot of mark code/label for fast rendering (resolved from mark_types).
    pub mark_code: String,
    pub mark_label: String,
    pub mark_category: MarkCategory,
    pub counts_in_average: bool,
    pub numeric_value: Option<Decimal>,
    pub weight: Decimal,
    pub work_type: WorkType,
    pub topic: String,
    pub comment: String,
    pub teacher: String,
    pub source: RecordSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttendanceStatus {
    Absent,
    Excused,
    Present,
    Custom(String),
}

impl AttendanceStatus {
    pub fn as_db_str(&self) -> String {
        match self {
            Self::Absent => "absent".into(),
            Self::Excused => "excused".into(),
            Self::Present => "present".into(),
            Self::Custom(code) => format!("custom:{code}"),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "absent" | "н/б" | "nb" => Some(Self::Absent),
            "excused" | "уваж." | "уваж" => Some(Self::Excused),
            "present" | "прис." | "прис" => Some(Self::Present),
            other if other.starts_with("custom:") && other.len() > 7 => {
                Some(Self::Custom(other[7..].to_string()))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttendanceEntry {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub subject_id: Uuid,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub status: AttendanceStatus,
    pub mark_type_id: Uuid,
    pub reason: String,
    pub comment: String,
    pub source: RecordSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// Kanban, Tasks, Checklists, Tags & Badges
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub name: String,
    pub description: String,
    pub color: String,
    pub sort_order: i32,
    pub is_archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KanbanColumn {
    pub id: Uuid,
    pub board_id: Uuid,
    pub name: String,
    pub color: String,
    pub sort_order: i32,
    pub is_collapsed: bool,
    pub is_archived: bool,
    pub wip_limit: Option<u32>,
    pub is_completed_column: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    #[default]
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

impl TaskPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "none" | "без приоритета" => Some(Self::None),
            "low" | "низкий" => Some(Self::Low),
            "medium" | "средний" => Some(Self::Medium),
            "high" | "высокий" => Some(Self::High),
            "critical" | "критический" => Some(Self::Critical),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> i32 {
        *self as i32
    }

    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::Low,
            2 => Self::Medium,
            3 => Self::High,
            4 => Self::Critical,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorityStyle {
    pub priority: TaskPriority,
    pub custom_label: String,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub id: Uuid,
    pub task_id: Uuid,
    pub title: String,
    pub is_completed: bool,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub name: String,
    pub color: String,
    pub icon: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Badge {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub name: String,
    pub color: String,
    pub icon: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub board_id: Uuid,
    pub column_id: Uuid,
    pub subject_id: Option<Uuid>,
    pub title: String,
    pub description: String,
    pub priority: TaskPriority,
    pub color: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub due_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub notes: String,
    pub sort_order: i32,
    pub is_archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<Tag>,
    pub badges: Vec<Badge>,
    pub checklist: Vec<ChecklistItem>,
    pub linked_grade_ids: Vec<Uuid>,
}

impl Task {
    pub fn is_completed(&self) -> bool {
        self.completed_at.is_some()
    }

    pub fn is_overdue(&self, now: DateTime<Utc>) -> bool {
        !self.is_archived && self.completed_at.is_none() && self.due_at.is_some_and(|due| due < now)
    }

    pub fn checklist_progress(&self) -> (usize, usize) {
        let done = self.checklist.iter().filter(|i| i.is_completed).count();
        (done, self.checklist.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGradeLink {
    pub task_id: Uuid,
    pub grade_id: Uuid,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Notifications, Reminders, Holidays & Activity Log
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    DeadlineUpcoming,
    TaskOverdue,
    Reminder,
    McpAction,
    System,
}

impl NotificationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DeadlineUpcoming => "deadline_upcoming",
            Self::TaskOverdue => "task_overdue",
            Self::Reminder => "reminder",
            Self::McpAction => "mcp_action",
            Self::System => "system",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "deadline_upcoming" => Self::DeadlineUpcoming,
            "task_overdue" => Self::TaskOverdue,
            "reminder" => Self::Reminder,
            "mcp_action" => Self::McpAction,
            _ => Self::System,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationItem {
    pub id: Uuid,
    pub profile_id: Option<Uuid>,
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<Uuid>,
    pub is_read: bool,
    pub is_dismissed: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderOffset {
    AtDue,
    OneHourBefore,
    OneDayBefore,
    ThreeDaysBefore,
    OneWeekBefore,
    CustomMinutes(i64),
}

impl ReminderOffset {
    pub fn minutes(&self) -> i64 {
        match self {
            Self::AtDue => 0,
            Self::OneHourBefore => 60,
            Self::OneDayBefore => 60 * 24,
            Self::ThreeDaysBefore => 60 * 24 * 3,
            Self::OneWeekBefore => 60 * 24 * 7,
            Self::CustomMinutes(m) => (*m).max(0),
        }
    }

    pub fn from_minutes(minutes: i64) -> Self {
        match minutes {
            0 => Self::AtDue,
            60 => Self::OneHourBefore,
            1440 => Self::OneDayBefore,
            4320 => Self::ThreeDaysBefore,
            10080 => Self::OneWeekBefore,
            other => Self::CustomMinutes(other.max(0)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: Uuid,
    pub task_id: Uuid,
    pub remind_at: DateTime<Utc>,
    pub offset_minutes: i64,
    pub is_fired: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holiday {
    pub id: Uuid,
    pub profile_id: Option<Uuid>,
    pub date: NaiveDate,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityLogEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub client_identity: Option<String>,
    pub tool_name: Option<String>,
    pub profile_id: Option<Uuid>,
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    pub operation_type: String,
    pub summary: String,
    pub result_status: String,
    pub source: RecordSource,
    pub correlation_id: String,
    pub undo_payload: Option<serde_json::Value>,
    pub undone: bool,
}

// ============================================================================
// Global Search
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchEntityType {
    Subject,
    Grade,
    Attendance,
    Task,
    Tag,
    Badge,
}

impl SearchEntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Subject => "subject",
            Self::Grade => "grade",
            Self::Attendance => "attendance",
            Self::Task => "task",
            Self::Tag => "tag",
            Self::Badge => "badge",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "subject" => Some(Self::Subject),
            "grade" => Some(Self::Grade),
            "attendance" => Some(Self::Attendance),
            "task" => Some(Self::Task),
            "tag" => Some(Self::Tag),
            "badge" => Some(Self::Badge),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub entity_type: SearchEntityType,
    pub entity_id: Uuid,
    pub profile_id: Uuid,
    pub subject_id: Option<Uuid>,
    pub date: Option<NaiveDate>,
    pub title: String,
    pub snippet: String,
}

// ============================================================================
// Settings & Preferences
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    Ru,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WeekStart {
    #[default]
    Monday,
    Sunday,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AverageMode {
    #[default]
    Arithmetic,
    Weighted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AveragePrecision {
    One = 1,
    #[default]
    Two = 2,
    Three = 3,
}

impl AveragePrecision {
    pub fn decimals(&self) -> u32 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
    #[default]
    DdMmYyyy,
    YyyyMmDd,
    MmDdYyyy,
}

impl DateFormat {
    pub fn format_date(&self, d: NaiveDate) -> String {
        match self {
            Self::DdMmYyyy => d.format("%d.%m.%Y").to_string(),
            Self::YyyyMmDd => d.format("%Y-%m-%d").to_string(),
            Self::MmDdYyyy => d.format("%m/%d/%Y").to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum McpPermissionMode {
    ReadOnly,
    #[default]
    ReadWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum McpTransportMode {
    #[default]
    Stdio,
    StreamableHttp,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkColorPalette {
    pub grade_5: String,
    pub grade_4: String,
    pub grade_3: String,
    pub grade_2: String,
    pub grade_1: String,
    pub absent: String,
    pub excused: String,
    pub pass: String,
    pub fail: String,
}

impl Default for MarkColorPalette {
    fn default() -> Self {
        Self {
            grade_5: "#2E7D32".into(),
            grade_4: "#00897B".into(),
            grade_3: "#D97706".into(),
            grade_2: "#EA580C".into(),
            grade_1: "#DC2626".into(),
            absent: "#E11D48".into(),
            excused: "#2563EB".into(),
            pass: "#0D9488".into(),
            fail: "#B91C1C".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    // Active state
    pub active_profile_id: Option<Uuid>,
    pub language: Language,
    // Appearance
    pub theme_mode: ThemeMode,
    pub accent_color: String,
    pub ui_scale: f32,
    pub font_size_delta: i32,
    pub compact_mode: bool,
    pub reduced_motion: bool,
    pub mark_colors: MarkColorPalette,
    // Journal
    pub week_start: WeekStart,
    pub highlight_weekends: bool,
    pub highlight_holidays: bool,
    pub date_format: DateFormat,
    pub average_precision: AveragePrecision,
    pub average_mode: AverageMode,
    // Kanban
    pub kanban_compact_cards: bool,
    pub kanban_show_completed: bool,
    pub kanban_default_sort: String,
    pub kanban_show_subject: bool,
    pub kanban_show_deadline: bool,
    pub kanban_show_tags: bool,
    pub kanban_show_checklist: bool,
    pub reminders_enabled: bool,
    pub os_notifications_enabled: bool,
    // MCP
    pub mcp_transport: McpTransportMode,
    pub mcp_http_port: u16,
    pub mcp_http_autostart: bool,
    pub mcp_permission_mode: McpPermissionMode,
    pub mcp_allow_destructive: bool,
    // Backups
    pub auto_backup_enabled: bool,
    pub auto_backup_interval_hours: u32,
    pub auto_backup_keep_count: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            active_profile_id: None,
            language: Language::Ru,
            theme_mode: ThemeMode::System,
            accent_color: "#4A4459".into(),
            ui_scale: 1.0,
            font_size_delta: 0,
            compact_mode: false,
            reduced_motion: false,
            mark_colors: MarkColorPalette::default(),
            week_start: WeekStart::Monday,
            highlight_weekends: true,
            highlight_holidays: true,
            date_format: DateFormat::DdMmYyyy,
            average_precision: AveragePrecision::Two,
            average_mode: AverageMode::Arithmetic,
            kanban_compact_cards: false,
            kanban_show_completed: true,
            kanban_default_sort: "manual".into(),
            kanban_show_subject: true,
            kanban_show_deadline: true,
            kanban_show_tags: true,
            kanban_show_checklist: true,
            reminders_enabled: true,
            os_notifications_enabled: true,
            mcp_transport: McpTransportMode::Stdio,
            mcp_http_port: 38421,
            mcp_http_autostart: false,
            mcp_permission_mode: McpPermissionMode::ReadWrite,
            mcp_allow_destructive: false,
            auto_backup_enabled: true,
            auto_backup_interval_hours: 24,
            auto_backup_keep_count: 10,
        }
    }
}
