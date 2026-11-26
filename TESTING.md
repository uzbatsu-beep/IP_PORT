# План проверки IP / PORT Scanner

**Область:** стабильная версия [v0.4.0](https://github.com/uzbatsu-beep/IP_PORT/releases/tag/v0.4.0), Windows x64 — основной сценарий приёмки; CI — Windows/Linux/macOS. Этот файл является **планом проверки**, а не подтверждением, что каждый ручной сценарий выполнен.

## 1. Автоматические проверки

Запуск из корня репозитория:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
python3 scripts/check_docs.py
cargo build --release
```

В GitHub Actions `.github/workflows/ci.yml` используется Rust **1.95** (MSRV) и платформенная матрица Windows x64, Linux x64, macOS ARM64. Для каждой платформы выполняются Clippy, тесты и оптимизированная сборка. Файл `scripts/check_docs.py` проверяет относительные ссылки в Markdown **офлайн**, без сетевых обращений. Для релизов отдельно выполняется `.github/workflows/release.yml` (включая macOS Intel).

## 2. Матрица функциональной приёмки Windows

| ID | Действие | Ожидаемый результат | Статус |
|---|---|---|---|
| W-01 | Скачать Windows ZIP и `SHA256SUMS.txt`, сверить хэш через PowerShell | Полное совпадение контрольной суммы ZIP | Ручная проверка |
| W-02 | Первый запуск на Windows 10/11 x64 без прав администратора | Окно открывается; нет аварийного завершения | Ручная проверка |
| W-03 | Несколько раз открыть `Services database`, `Load file`, `Reset` | Нет падений; старые имена сервисов в ранее найденных результатах не искажаются | Ручная проверка |
| W-04 | В `Exclude targets` добавить IP, CIDR и диапазон; разрешить DNS-цель | Исключённые IP не попадают в TCP/UDP-пробы; неверное правило блокирует Start | Ручная проверка |
| W-05 | TCP по разрешённым тестовым IP и портам; выбрать интерфейс | Корректные `Open` и RTT; ошибки подключения агрегируются | Ручная проверка |
| W-06 | UDP по разрешённым тестовым портам | `Open|Filtered` остаётся неоднозначным; для него RTT не показан как измеренный | Ручная проверка |
| W-07 | Включить `Resolve reverse DNS` для адресов с PTR и без PTR | Корректно меняются счётчики resolved/no-name/error, без блокировки GUI | Ручная проверка |
| W-08 | Настроить мягкий порог PTR 500–30000 мс и проверить медленный DNS | Появляется предупреждение, но **не** ложное сообщение об отмене DNS | Ручная проверка |
| W-09 | Включить `Read local ARP / NDP cache` | Показан MAC только из системного кэша; не выполняются активные ARP/NDP-пробы | Ручная проверка |
| W-10 | Проверить IPv6 link-local на двух интерфейсах с совпадающими IP | Отображается неоднозначность; произвольный MAC не присваивается | Ручная проверка |
| W-11 | Импортировать официальные IEEE MA-L, MA-M, MA-S CSV | UI реагирует на действия; категории и производители определяются по более специфичному префиксу | Ручная проверка |
| W-12 | Закрыть/открыть программу после импортов | Локальный кэш IEEE восстановлен без загрузки из сети | Ручная проверка |
| W-13 | Импортировать испорченный CSV, затем `Reset OUI`, перезапустить | Ошибочный CSV не портит кэш; Reset удаляет сохранённые реестры | Ручная проверка |
| W-14 | Экспортировать CSV/JSON/HTML, открыть HTML офлайн | Нет ошибок; HTML экранирует опасные символы; метаданные OUI/PTR присутствуют при наличии | Ручная проверка |
| W-15 | `Flat` ⇄ `Group by Host`, сортировка/фильтр, 850+ уникальных узлов в контролируемом тесте | Не теряются фоновые запросы, интерфейс не зависает | Ручная проверка |

**Примечание по W-15:** не следует создавать массовый сетевой трафик в производственной сети, особенно АСУ ТП. Используйте эмулятор/лабораторную сеть и заранее согласованный интервал.

## 3. Что сохранять в отчёте об испытаниях

Укажите версию и SHA ZIP, Windows build, тип сети (без раскрытия конфиденциальной топологии), включённые опции, диапазон целей (обезличенно), ID сценария, фактический результат, время/воспроизведение, скриншот интерфейса, диагностическое сообщение. Для аварий фиксируйте `perfmon /rel` и код исключения; подробности в [OPERATIONS.md](OPERATIONS.md). Не прикладывайте секреты или чувствительные сетевые дампы.

## 4. Критерии допуска к следующему релизу

- CI успешен на актуальном commit SHA, Rust 1.95, Clippy и тесты зелёные.
- Все обязательные Windows-сценарии W-01—W-14 выполнены либо наглядно отмечены как исключения с обоснованием.
- В `main` нет одноразовых release-скриптов, открытых серьёзных дефектов и непроверенных изменений.
- GitHub Release содержит четыре архива и полный `SHA256SUMS.txt`, хэши проверены; [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md).
- Приёмка в реальном OT-сегменте проводится **только после разрешения владельца сети**, отдельно от лабораторной проверки.


## ENG-01.A

Use the pinned Rust 1.100.0 toolchain and `--locked` for Cargo check/clippy/test/build. Run the cross-platform preflight in [ENGINEERING.md](ENGINEERING.md). After repeated Windows failures, 20k performance and 100k stress tests are executed sequentially in separate processes via `python scripts/preflight.py --tests-only`; exact test discovery is required. The 20k test logs five fixed warm-up frames (including cold start) separately, then measures all 60 steady-state frames with mean <16 ms and P95 <25 ms unchanged. Cold-start times are not subject to those steady-state gates. The 100k functional assertions are unchanged. Optimized-profile benchmarking, more scenarios and JSON metrics remain ENG-01.B. The MSRV check remains an independent locked build on 1.95.0.


ENG-01.B: the existing isolated debug tests remain unchanged; optimized matrix is excluded from functional test command and executed explicitly by scripts/performance.py with release profile. Exact test discovery and report validation prevent false positives. See [PERFORMANCE.md](PERFORMANCE.md). Engineering scripts have independent unittest coverage.
