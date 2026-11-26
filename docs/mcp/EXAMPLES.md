# Oxidium MCP Natural-Language Scenarios (Verified End-to-End)

All 9 scenarios below are covered by automated integration tests (`crates/journal-mcp/tests/mcp_tests.rs`) executing real MCP JSON-RPC tool calls against the server:

1. **«Поставь мне 5 по философии за сегодня»**
   - `get_active_profile` → `find_subject(query="Философия")` → `add_grade(profile_id, subject_id, date=today, grade=5, idempotency_key=...)`.
2. **«Отметь, что вчера меня не было на физкультуре по уважительной причине»**
   - `find_subject(query="Физкультура")` → `add_attendance(profile_id, subject_id, date=yesterday, status="excused", reason="Уважительная причина")`.
3. **«Добавь лабораторную по базам данных, дедлайн в пятницу»**
   - `find_subject(query="Базы данных")` → `create_task(profile_id, subject_id, title="Лабораторная №4", deadline=friday_iso, priority="high", tags=["Лабораторная"])`.
4. **«Покажи мой средний балл по программированию»**
   - `find_subject(query="Программирование")` → `get_average_grade(profile_id, subject_id)`.
5. **«Какие задания у меня просрочены?»**
   - `get_overdue_tasks(profile_id)`.
6. **«Перемести лабораторную №4 в выполненные»**
   - `search_tasks(profile_id, query="Лабораторная №4")` → `complete_task(task_id)`.
7. **«Сколько пятёрок мне нужно, чтобы средний балл по философии стал 4.5?»**
   - `find_subject(query="Философия")` → `calculate_grade_forecast(profile_id, subject_id, target_average="4.5", assumed_grade=5)`.
8. **«Составь сводку моей успеваемости за ноябрь»**
   - `get_monthly_summary(profile_id, year=2026, month=11)` + `get_overall_statistics`.
9. **«Добавь все задания из переданного мной списка»**
   - `create_tasks_batch(items=[...], idempotency_key=...)`.
