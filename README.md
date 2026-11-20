# IP / PORT Scanner

> **v0.4.0 (PR #2, development branch):** scan exclusions (IPv4/IPv6, CIDR, ranges), optional background PTR lookups, and read-only ARP/NDP neighbor-cache MAC/OUI identification. These features are not part of the published v0.3.1 release.

### Windows MAC/OUI and PTR diagnostics (v0.4.x, PR #6)

On Windows, optional **Read local ARP / NDP cache** uses the OS IP Helper API `GetIpNetTable2`, reading IPv4 and IPv6 neighbor records without active ARP/NDP probes, launching PowerShell or parsing the locale-dependent `arp -a` text. Results are still limited to directly reachable neighbors in the OS cache. MAC-to-manufacturer attribution uses a deliberately small offline OUI dictionary; unknown and locally administered addresses are not guessed. Link-local IPv6 addresses may be ambiguous across interfaces.

When **Resolve reverse DNS (PTR)** is enabled, the Host identification section shows **resolved / no name / resolver errors**, last OS resolver error, number of processed/pending IPs and the queue backlog. Hover a host's IP in the results table to see its PTR state, elapsed resolver time, cached MAC and cache diagnostics. Missing PTR entries may be reported as errors by some operating systems, so **No PTR name** is only shown for explicit missing-name responses. The OS resolver has no guaranteed per-request timeout; it runs in two background threads, not the GUI. When more than 512 IPs arrive, pending lookups are retried rather than dropped.

### Scope controls in v0.4.0

Use **Exclude targets** in the sidebar to avoid contacting sensitive equipment: `192.168.1.1`, `10.0.0.0/24`, `192.168.1.200-254`, `2001:db8::/64`. Exclusions are evaluated after resolving hostname targets and before scheduling any port probe. Invalid exclusions and an entirely excluded scope block the start of a scan.

Under **Host identification**, enable Reverse DNS (PTR) and/or read-only neighbor-cache lookup if needed. PTR may query the configured DNS servers. ARP/NDP inspection does **not** send additional discovery traffic or require administrator privileges; only OS-cached neighbors may have MAC addresses. The built-in OUI map contains a small selection of vendors; unknown or randomized MAC addresses remain unattributed.

Scan profiles preserve these options. JSON and HTML exports include available `hostname`, `mac`, `oui` and `vendor` fields; original CSV v1 remains compatible. For operational-technology networks, always review the target and exclusion lists before starting a scan.


Кроссплатформенное приложение для сканирования IP-адресов, TCP- и UDP-портов с современным интерфейсом на Rust.

![Rust](https://img.shields.io/badge/Rust-1.95%2B-orange)
![egui](https://img.shields.io/badge/GUI-egui%200.37-blue)
![License](https://img.shields.io/badge/license-MIT-green)

## Возможности

- Сканирование IPv4/IPv6-адресов, CIDR-подсетей, диапазонов IPv4/IPv6, а также DNS-имён хостов (FQDN, `localhost`) с асинхронным Dual-Stack резолвингом (A/AAAA)
- Поддержка протоколов TCP и UDP с переключением в один клик
- RFC-совместимые payload-пробы для стандартных UDP-служб (DNS 53, NTP 123, SNMP 161, NetBIOS 137)
- Чёткое разделение исходов: подтверждённый `Open` vs неоднозначный `Open|Filtered` (бейджи `?` с тултипом)
- Гибкий выбор портов: диапазоны (`1-1024`), списки (`22,80,443`), сервисные пресеты
- Асинхронный движок на Tokio: параллельные подключения с контролем конкурентности (рекомендация $\le 64$ для UDP)
- Привязка к локальному сетевому интерфейсу / IP-адресу источника (Network Interface Binding) для TCP и UDP
- Настраиваемые таймауты подключения и количество воркеров
- Живой вывод открытых портов, RTT-задержки и предполагаемого сервиса по номеру порта
- Опциональный захват баннеров (Service banner grabbing) с отдельным таймаутом, буфером до 2048 байт, автоматическим извлечением заголовков веб-страниц (HTTP Title Grabbing) и подтверждением сервиса (`✔`)
- Интерактивная визуализация задержки RTT (Min, Avg, Median, p95, Max) с адаптивной цветной гистограммой и сводным прогресс-баром
- Контекстные быстрые действия и панель-инспектор: открытие в браузере, генерация curl/ssh/db-команд, фильтрация по хосту и постановка в цели
- Два режима отображения результатов: классическая плоская таблица (`Flat`) и сворачиваемая группировка по хостам (`Group by Host`)
- Сводные счётчики отказов соединения (ICMP/TCP RST), таймаутов, `Open|Filtered` и сетевых ошибок
- Прогресс сканирования, статистика, отмена в любой момент; хранение ограничено 100 000 открытых результатов
- Фильтрация по адресу, порту, сервису, протоколу или состоянию; сортировка и контекстные действия
- Асинхронный фоновый экспорт снимка результатов в CSV / JSON (схема v1) и интерактивный автономный HTML-отчёт с поиском, фильтрацией и карточками метрик
- Сохранение и загрузка конфигурационных профилей сканирования в JSON (схема v1) с валидацией
- Современная тёмная тема и адаптивный layout

## Стек

| Компонент | Технология |
|-----------|------------|
| Язык | Rust 2024 edition |
| GUI | `eframe` + `egui` (нативный рендеринг через `glow`) |
| Асинхронность | `tokio` (multi-thread runtime) |
| Парсинг сетей | `ipnet` |
| Диалоги файлов | `rfd` |
| Отмена задач | `tokio-util` (`CancellationToken`) |

## Архитектура

```
src/
├── main.rs          # точка входа, инициализация eframe
├── app.rs           # состояние UI и обработка событий
├── ui/
│   ├── mod.rs       # регистрация панелей интерфейса
│   ├── quick_actions.rs # контекстные быстрые действия и генераторы команд
│   ├── sidebar.rs   # форма целей, портов, протокола и настроек
│   ├── results.rs   # таблица открытых портов и статусов
│   ├── rtt_view.rs  # расчёт статистики и визуализация распределения задержки (RTT)
│   └── theme.rs     # современное тёмное оформление
├── scanner/
│   ├── mod.rs       # запуск и управление сканированием
│   ├── interfaces.rs # обнаружение локальных сетевых интерфейсов
│   ├── presets.rs   # пресеты портов для TCP и UDP со сжатием диапазонов
│   ├── targets.rs   # разбор целей: IP, CIDR, диапазоны портов
│   ├── probe.rs     # TCP-connect и маршрутизация проб с таймаутом
│   └── udp.rs       # UDP-зондирование, payload-пробы и классификация ошибок
└── export.rs        # сохранение результатов в CSV / JSON
```

Сканирование выполняется в отдельном Tokio-рантайме. События передаются в UI через канал
`mpsc`, а `CancellationToken` используется для остановки текущей сессии. Подробнее о текущем
устройстве и ограничениях — в [архитектуре](ARCHITECTURE.md).

## Сборка и запуск

```bash
# отладочная сборка
cargo run

# релизная сборка
cargo build --release

# запуск всех тестов
cargo test
```

Подробные инструкции по сборке, системным зависимостям и установке бинарных пакетов для **Windows**, **Linux** и **macOS** описаны в [руководстве по установке](PACKAGING.md).
История версий и список изменений документируются в [CHANGELOG.md](CHANGELOG.md).

## Использование

1. Введите цели в поле **Targets** — по одной на строку: IP, CIDR, диапазон адресов (например, `10.0.0.1-10.0.0.20`, `::1-::5`) или DNS-имена хостов (`localhost`, `example.com`, `scanme.nmap.org`). Строки, начинающиеся с `#`, игнорируются. Синтаксис доменов проверяется мгновенно без блокировки интерфейса; предпросмотр показывает число распознанных доменов.
2. Укажите порты: `22,80,443`, `1-1024` или выберите пресет (**Common**, **Web**, **Database**).
3. Выберите протокол сканирования: **TCP** или **UDP**. Для UDP рекомендуется конкурентность $\le 64$ во избежание потерь на уровне ограничения частоты ICMP в ядре ОС.
4. Настройте таймаут и конкурентность при необходимости. Кнопки **Save profile...** и **Load profile...** сохраняют/загружают цели, порты, таймаут, конкурентность и протокол в версионируемом JSON-профиле; загружаемый профиль валидируется до применения.
5. Нажмите **Start scan** или используйте **Ctrl+Enter** (на macOS — **Cmd+Enter**). При наличии доменных имён Tokio-рантайм перед стартом воркеров асинхронно разрешает все A и AAAA записи (с таймаутом 3 с и немедленной отменой), дедуплицируя адреса. Найденные порты появляются в таблице по мере обнаружения:
   - В режиме **TCP**: выводятся подтверждённые открытые порты с RTT соединения.
   - В режиме **UDP**: порты с полученным ответом выводятся как `open` с RTT, а молчащие порты — как `open|filtered` с индикатором `? (open|filtered)` и поясняющим тултипом (в соответствии с RFC 768 / RFC 1812 отсутствие ответа может означать фильтрацию пакетов).
   Доступна полная клавиатурная навигация: стрелки **Up / Down**, **Home**, **End** для перемещения по строкам, **Enter / Space** для открытия web-портов в браузере или копирования, **Filter** (хоткей **Ctrl/Cmd+F**), заголовки столбцов для сортировки и контекстное меню по правому клику (с автоматическим выделением строки) для копирования адресов/результатов.
6. **Stop** запрашивает отмену. **Clear** удаляет уже собранные результаты, но не останавливает сканирование — новые результаты продолжат поступать. Меню **Export ▾** сохраняет снимок всех собранных результатов (без фильтра/сортировки) в CSV или JSON с timestamp-именем по умолчанию (`scan_results_YYYYMMDD_HHMMSS.{ext}`). Запись выполняется в фоне; после сохранения появляется кликабельная ссылка для перехода к файлу в проводнике.

Приложение поддерживает TCP-connect и UDP-сканирование со специализированными payload-пробами.
Отказ соединения (TCP RST или ICMP Port Unreachable), таймаут и сетевые ошибки отображаются как
сводные счётчики, без хранения адреса/порта каждой неудачной пробы. В памяти сохраняются только открытые
и `open|filtered` результаты, максимум 100 000 строк; остальные исходы учитываются отдельно, но не попадают в таблицу и экспорт.
Доменные имена разрешаются асинхронно перед началом зондирования с извлечением всех IPv4 и IPv6 адресов.
Лимит — 65 536 уникальных адресов на запуск; IPv4 CIDR ограничен `/16` и уже, IPv6 CIDR — `/120`
и уже. Диапазоны адресов поддерживаются для IPv4 и IPv6. Идентификаторы зон IPv6 (`%zone`) отклоняются
с явным диагностическим сообщением.

## Разработка

Цели, приоритеты и критерии готовности этапов описаны в [плане разработки](ROADMAP.md).
Текущее устройство и технические ограничения описаны в [архитектуре](ARCHITECTURE.md).
После завершения работы обновляйте эти документы вместе с кодом, чтобы сохранять контекст. Проверки базового среза (96 тестов, Windows, 2026-11-18); в v0.3.1 добавлены регрессионные тесты: `cargo fmt --check`, `cargo test` и `cargo clippy --all-targets -- -D warnings`.

## Лицензия

MIT
