# Руководство по участию в разработке (Contributing Guidelines)

Мы рады любому вкладу в развитие **IP / PORT Scanner**! Вы можете помочь проекту сообщением об ошибке, предложением новой функции или отправкой Pull Request.

---

## 1. Начало работы

### Требования к окружению
- **Rust:** rustup; разработка и релиз на `1.100.0` из `rust-toolchain.toml`, MSRV `1.95.0`, edition 2024.
- **Python:** 3.10+ для preflight и проверки документации.
- **Cargo:** встроенный менеджер пакетов.
- **Git**

### Клонирование и запуск
```bash
git clone https://github.com/uzbatsu-beep/IP_PORT.git
cd IP_PORT
cargo run --locked
```

---

## 2. Стандарты качества и проверки

Перед отправкой изменений выполните `cargo fmt --all`, затем `powershell -File scripts/preflight.ps1` (Windows) или `bash scripts/preflight.sh` (Linux/macOS). Можно использовать `python3 scripts/preflight.py`. Preflight выполняет format/docs/check/clippy/test/release и останавливается при первой ошибке. См. [ENGINEERING.md](ENGINEERING.md) и [AGENTS.md](AGENTS.md).

Отдельные команды:

1. **Форматирование кода:**
   ```bash
   cargo fmt --all --check
   ```
2. **Статический анализ (Clippy):** Код должен компилироваться без единого предупреждения:
   ```bash
   cargo clippy --locked --all-targets -- -D warnings
   ```
3. **Модульные и интеграционные тесты:**
   ```bash
   cargo test --locked --all-targets
   ```

4. **Проверка всей Markdown-документации (локальные ссылки):**
   ```bash
   python3 scripts/check_docs.py
   ```

Также выполняются `cargo check --locked --all-targets` и `cargo build --locked --release`. Все команды должны завершаться с кодом `0`. CI дополнительно проверяет `Rust 1.95` (MSRV) и release-сборки Windows/Linux/macOS. Тесты GUI/сетевой логики в CI не заменяют ручную проверку Windows в тестовом сегменте.

---

## 3. Архитектурные принципы проекта

При добавлении новых функций просим соблюдать ключевые принципы, зафиксированные в [ARCHITECTURE.md](ARCHITECTURE.md):

- **Не блокировать UI-поток:** сетевой ввод-вывод (I/O), DNS-резолвинг и тяжелые вычисления должны выполняться асинхронно в Tokio-рантайме или в фоновом потоке.
- **Zero-Privilege Portability:** избегать требований повышенных привилегий (root/admin/RAW sockets), если задачу можно решить через стандартные сокеты ОС или системные API.
- **Контроль памяти:** результаты открытых портов ограничены 100 000 строками; отклонённые и таймаут-пробы хранятся только в виде агрегированных счетчиков.
- **Кэширование представления:** таблица результатов использует O(N) кэширование видимых строк и групп хостов, не выполняя повторной фильтрации на холостых кадрах отрисовки egui.

---

## 4. Процесс отправки Pull Request

1. Форкните репозиторий и создайте тематическую ветку (`git checkout -b feat/my-new-feature` или `fix/my-bugfix`).
2. Внесите изменения с понятными атомарными коммитами по соглашению [Conventional Commits](https://www.conventionalcommits.org/ru/v1.0.0/) (`feat: ...`, `fix: ...`, `docs: ...`, `refactor: ...`).
3. При необходимости обновите документацию:
   - [CHANGELOG.md](CHANGELOG.md) — добавьте описание в секцию `[Unreleased]`.
   - [ROADMAP.md](ROADMAP.md) — отметьте выполненный пункт или обновите задачи.
   - [ARCHITECTURE.md](ARCHITECTURE.md) — зафиксируйте новые архитектурные ограничения.
4. Убедитесь, что все проверки (`cargo fmt`, `cargo clippy`, `cargo test`) проходят успешно.
5. Отправьте Pull Request в ветку `main` основного репозитория, заполнив форму по предложенному шаблону.

---

## 5. Обязательные условия для сетевых и UI-изменений

- Область целей должна быть проверена **после** DNS A/AAAA и **до** планирования портовых проб; запрещённый IP не должен попадать к TCP/UDP воркерам. Для новых парсеров добавляйте тесты некорректного ввода и краевых случаев.
- Никакие блокирующие операции (DNS, чтение больших CSV, запись файлов, ожидание ОС) не должны выполняться в кадре `egui`; данные, полученные фоновой задачей, должны быть явно отражены в статусе UI.
- Особо проверяйте ограничения: UDP `Open|Filtered` ≠ `Open`, для такого исхода отсутствует измеренный RTT; `PTR warning` не равен жёсткой отмене DNS-вызова; MAC от OS ARP/NDP-кэша не является удалённым сканированием.
- Для Windows-изменений приложите результат теста на Windows runner и, по возможности, инструкции ручной проверки двух интерфейсов и восстановления IEEE OUI.
- В Issue/PR не размещайте секреты, учётные данные, чувствительные адресные планы, дампы OT-сетей или реальные данные сотрудников. Для уязвимостей используйте [SECURITY.md](SECURITY.md).

## 6. Дисциплина релизов

Релизы создаются **из проверенного коммита по тегу `vX.Y.Z`**, версия в `Cargo.toml` должна совпадать с тегом. Workflow проверяет все четыре платформенных архива до публикации SHA-256. Одноразовые workflow и временные файлы подготовки не должны попадать в `main`. Проверяйте [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) до любого выпуска.

Не объединяйте PR автоматически: запросите согласование сопровождающего, проверьте всю CI-матрицу и конфликты. Для документационных изменений актуализируйте README/ARCHITECTURE/ROADMAP/CHANGELOG/SECURITY и связанные руководства.

For CI-equivalent test execution use `python scripts/preflight.py --tests-only`: all functional tests followed by both isolated load tests. Test discovery fails closed if either benchmark is missing. See [TESTING.md](TESTING.md) for the distinction between cold-start and steady-state timing.

ENG-01.B: full preflight includes optimized Performance QA. CI additionally runs mandatory Dependency Audit and Quality Gate; see [DEPENDENCIES.md](DEPENDENCIES.md) and [PERFORMANCE.md](PERFORMANCE.md).
