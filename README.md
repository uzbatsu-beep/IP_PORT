# IP / PORT Scanner

**Актуальный стабильный релиз: [v0.4.0](https://github.com/uzbatsu-beep/IP_PORT/releases/tag/v0.4.0)** · Windows x64 / Linux x64 / macOS Apple Silicon и Intel · лицензия MIT.

IP / PORT Scanner — настольный TCP/UDP-сканер сетевых адресов и портов на Rust/egui. В версии 0.4.0 доступны исключения целей, обратное разрешение PTR, локальная идентификация MAC/OUI и загрузка официальных IEEE CSV-реестров с сохранением между запусками.

**Скачать:** [Windows ZIP](https://github.com/uzbatsu-beep/IP_PORT/releases/download/v0.4.0/ip_port_scanner-v0.4.0-x86_64-pc-windows-msvc.zip) · [все платформы](https://github.com/uzbatsu-beep/IP_PORT/releases/tag/v0.4.0) · [SHA256SUMS.txt](https://github.com/uzbatsu-beep/IP_PORT/releases/download/v0.4.0/SHA256SUMS.txt).

> **Безопасность:** запускайте сканирование только по разрешённым адресам. В сегментах АСУ ТП / OT сначала определите допустимую область, исключите PLC, роботов и другое критичное оборудование, согласуйте нагрузку и по умолчанию оставляйте сбор баннеров выключенным. TCP/UDP-зондирование является активным сетевым действием. PTR может отправлять DNS-запросы; чтение уже существующего ARP/NDP-кэша дополнительных зондирующих пакетов не отправляет.

### Быстрый старт v0.4.0

1. Распакуйте архив из GitHub Releases. Перед запуском сравните SHA-256 файла с `SHA256SUMS.txt`; пример приведён в [PACKAGING.md](PACKAGING.md).
2. В **Targets** введите адреса или CIDR, в **Ports** — список портов, выберите TCP/UDP и при необходимости `Network Interface`.
3. Для чувствительных хостов настройте **Exclude targets**: `192.168.1.1`, `10.0.0.0/24`, `192.168.1.200-254`, `2001:db8::/64`. Правила применяются **после DNS A/AAAA** и **до TCP/UDP-проб**.
4. В **Host identification** при необходимости включите **Reverse DNS (PTR)** и **Read local ARP / NDP cache**. Для более полного определения производителя воспользуйтесь **Import IEEE CSV...**. Официальные MA-L, MA-M, MA-S сохраняются локально и восстанавливаются при запуске; **Reset OUI** сбрасывает импорт.
5. Нажмите **Start scan**, просматривайте `Flat` / `Group by Host` и сохраните результат в CSV, JSON или HTML.

**Что означают ограничения:** `Open|Filtered` в UDP не доказывает открытый порт и не имеет измеренного RTT; MAC удалённого узла обычно нельзя получить из локального кэша; одинаковые IPv6 link-local на разных интерфейсах остаются неоднозначными. `PTR warning (ms)` — **мягкий порог предупреждения**, а не гарантированный таймаут или принудительная отмена системного DNS-вызова.

**Согласованное направление:** универсальная карточка промышленного устройства, локальная модель активов, история и многоканальная идентификация. Это **план развития**, а не готовые функции v0.4.0. [Принятые и отложенные решения](DECISIONS.md) · [Импорт GSDML/EDS/ESI/IODD](DEVICE_DESCRIPTIONS.md) · [Индустриальные стандарты 2026](INDUSTRY_TRENDS_2026.md).

Руководства: [архитектура](ARCHITECTURE.md), [план развития](ROADMAP.md), [установка и проверка архивов](PACKAGING.md), [сценарии тестирования](TESTING.md), [практическая эксплуатация](OPERATIONS.md), [IEEE OUI](OUI_DATA.md), [аудит репозитория](AUDIT.md).

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
├── app.rs           # состояние UI, фоновые задачи, очереди и результаты
├── host_info.rs     # PTR, ARP/NDP и интерфейсные записи
├── oui_registry.rs  # IEEE MA-L/MA-M/MA-S и longest-prefix matching
├── oui_store.rs     # локальный персистентный OUI-кэш с восстановлением
├── profile.rs       # сохранение настроек сканирования
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
│   ├── targets.rs   # разбор целей: IP, CIDR, DNS и диапазоны
│   ├── scope.rs     # правила исключения адресов
│   ├── services.rs  # встроенная и внешняя база названий сервисов
│   ├── probe.rs     # TCP-connect и маршрутизация проб с таймаутом
│   └── udp.rs       # UDP-зондирование, payload-пробы и классификация ошибок
└── export.rs        # CSV / JSON / автономный HTML
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
После каждого изменения обновляйте документацию вместе с кодом и запускайте `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` и `python3 scripts/check_docs.py`. Матричный CI проверяет Windows, Linux и macOS; подробности приведены в [TESTING.md](TESTING.md).

## Лицензия

MIT


## Appearance (UI-01)

Open **Appearance** above the scan controls. Select Light or Dark independently of one of eight accent palettes; changes are applied immediately and saved on selection. **Reset appearance** restores Dark / Classic Blue. Settings are stored as `appearance.json` beside the application's OUI directory, not in scan profiles. If reading or saving fails, the appearance panel shows the error and offers Retry save. Existing scan state is not reset by theme changes. See [UI appearance](UI_APPEARANCE.md) for paths and acceptance tests.


## Shared UI components (UI-02)

The scanner uses reusable theme-aware controls, semantic status text, metrics, panel surfaces and empty states. RTT severity colors adapt to light/dark while retaining their meaning. See [UI design system](UI_DESIGN_SYSTEM.md).


## Engineering Workspace (UI-03.1)

The startup view groups observations by IP (Devices); Ports retains the existing flat table. Use the left icon rail for Discovery, Devices or Settings (appearance). Wide windows show a right inspector for a selected endpoint; narrow windows expose the same details in a collapsible region. The RU/EN selector translates the new shell, not all existing controls yet. See [UI_WORKSPACE.md](UI_WORKSPACE.md).


UI-03.2 adds six functional routes and collapsible vector navigation, persistent shell language, and an offline [industrial protocol reference](INDUSTRIAL_PROTOCOLS.md). No new active discovery adapters are enabled.


UI-03.3 adds a global two-level toolbar: start/stop, profiles, export, scan context, theme and settings. Ctrl/Cmd+O loads a profile and Ctrl/Cmd+S saves one when no scan is active. Reports uses the same command dispatcher; Discovery retains configuration, not duplicated controls.


Upcoming v0.5.0 (development): Discovery adds General / Industrial / Custom, curated catalog-derived port candidates by manufacturer/family/protocol, source links, and enhanced ScopePreview. JSON profile v1 remains compatible. The published stable release remains v0.4.0 until the v0.5.0 release workflow completes.
