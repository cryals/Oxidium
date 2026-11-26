# Oxidium MCP Resources & Prompts

Oxidium implements **MCP Resources** (`resources/list`, `resources/templates/list`, `resources/read`) and **MCP Prompts** (`prompts/list`, `prompts/get`) alongside its 80 Tools.

## Resources

| URI / Template | MIME Type | Contents |
| --- | --- | --- |
| `oxidium://profiles` | `application/json` | Active profile ID and list of all profiles |
| `oxidium://profiles/{profile_id}/subjects` | `application/json` | Subjects in the specified profile |
| `oxidium://profiles/{profile_id}/mark-types` | `application/json` | Built-in and custom mark types |
| `oxidium://profiles/{profile_id}/academic-periods` | `application/json` | Configured academic periods |
| `oxidium://profiles/{profile_id}/tasks/active` | `application/json` | Incomplete active Kanban tasks |
| `oxidium://schema/reference` | `application/json` | Entity schema, mark categories, work types, and prompt-injection data notice |

## Prompts (7)

1. `daily_academic_summary` (`date?`, `profile_id?`) — daily grades, attendance, overdue & upcoming tasks brief.
2. `weekly_academic_review` — weekly progress review and subjects below target.
3. `monthly_performance_review` (`year?`, `month?`, `profile_id?`) — monthly summary + period comparison.
4. `upcoming_assignments` — deadline triage grouped by urgency.
5. `grade_improvement_plan` (`subject`, `target_average`) — exact forecast calculation + study plan.
6. `academic_workload_planning` — balanced weekly study plan.
7. `import_academic_notes` (`notes`) — safe preview workflow for transferring unstructured notes (e.g., Telegram Saved Messages) into Oxidium.
