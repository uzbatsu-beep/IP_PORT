# IP_PORT — импорт промышленных описаний устройств

Дата исследования: **2026-11-21**. Статус: **проектируемая возможность, пока не реализована**. Согласованы промышленная специализация, универсальная карточка и модель активов; конкретные парсеры, форматы и очередность работ ещё не утверждены.

## 1. Для чего это нужно

Файл описания от производителя характеризует **тип/модель** оборудования и доступные аппаратные модули, параметры и свойства. Он не сообщает автоматически, какая физическая единица подключена к сети сейчас.

Потенциальный сценарий: инженер импортирует GSDML для PROFINET IO-device или IODD для датчика. IP_PORT создаёт запись каталога; данные фактического сканирования, проекта, IO-Link master, EtherCAT master либо явно разрешённой идентификации предоставляют **наблюдаемые** идентификаторы. Только затем выполняется корреляция между наблюдением и записью каталога.

## 2. Основные форматы описания

| Формат | Экосистема | Типовое содержимое | Как можно связать с наблюдением |
|---|---|---|---|
| **GSDML (XML)** | PROFINET | Vendor ID, Device ID, Device Access Point, модули/подмодули, параметры, диагностика | По фактическому Vendor ID/Device ID из PROFINET-проекта, мастера или разрешённого метода идентификации |
| **GSD (платформенный формат)** | PROFIBUS | Ident Number, профиль станции, опции обмена | По данным PROFIBUS мастера/шлюза или загруженного проекта |
| **EDS (CIP)** | EtherNet/IP / DeviceNet | Vendor ID, Device Type, Product Code, Revision, параметры и assemblies | По CIP Identity Object либо офлайн-проекту |
| **EDS (CANopen)** | CANopen / CANopen FD | Identity Object 0x1018, object dictionary, параметры и функции | По EDS + фактическим идентификаторам узла от CANopen master/шлюза |
| **ESI (XML)** | EtherCAT | Vendor ID, Product Code, Revision, PDO, SyncManager, модули | По фактическим описаниям slave из EtherCAT master/инженерной конфигурации |
| **IODD (XML)** | IO-Link | Vendor ID, Device ID, параметры, единицы, data types, тексты и диагностика | По паре Vendor ID/Device ID, полученной от IO-Link master, либо из инженерного проекта |
| **CSP+** | CC-Link | Описание параметров/коммуникаций и профиля устройства | По идентификаторам производителя/устройства, доступным через мастер/проект |
| **FDI/EDD** | HART / PROFIBUS PA / FOUNDATION Fieldbus | Описание параметров, функций и диагностических данных КИП | Через шлюз или инженерный инструмент; не прямым IP scan |

**Осторожно с EDS:** общая аббревиатура не означает одинаковый синтаксис CIP и CANopen. Следует использовать разные парсеры и отдельные области идентификаторов.

Официальные источники:
- [PROFINET GSDML — PI](https://sea.profibus.com/downloads-and-media/gsdml-gsdx-specification-for-profinet)
- [ODVA EtherNet/IP developers' guide (EDS/Identity)](https://www.odva.org/wp-content/uploads/2020/05/PUB00213R0_EtherNetIP_Developers_Guide.pdf)
- [EtherCAT Technology Group — ESI](https://www.ethercat.org/en/downloads/downloads_6a46d45ea33c47ecb2bb2686bba963ec.htm)
- [IO-Link IODD](https://io-link.com/technology/iodd)
- [OPC UA for IO-Link Masters/Devices](https://reference.opcfoundation.org/specs/OPC-30120/full)

## 3. Пример: PROFINET IO-Link Master + четыре датчика

**Исходные данные**: обнаружен IP узла PROFINET; инженер импортировал GSDML мастера и четыре IODD. Это **ещё не доказывает**, что четыре устройства подключены. Сведения о фактическом составе портов должны прийти из конфигурации проекта, read-only данных мастера или другого проверенного источника.

**Модель связей:**
```text
Asset: IO-Link Master (есть IP/MAC, PROFINET)
  NetworkInterface: Ethernet 1
  DeviceDescription: GSDML, Vendor ID + Device ID
  LogicalDevice/Port 1 → Asset: pressure sensor (IODD + фактический ID)
  LogicalDevice/Port 2 → Asset: temperature sensor (IODD + фактический ID)
  LogicalDevice/Port 3 → Asset: distance sensor (IODD + фактический ID)
  LogicalDevice/Port 4 → Asset: valve controller (IODD + фактический ID)
```

Физические датчики могут не иметь IP: IP_PORT хранит их как **дочерние активы/логические компоненты** и связывает с сетевым мастером. Если известно только описание датчика, запись остаётся в **каталоге типов**, а не среди подтверждённых физических активов.

## 4. Проектируемый конвейер

```text
Локальный файл пользователя
 → определение типа и проверка версии/схемы
 → ограниченный по объёму офлайн-парсинг
 → нормализация в DeviceDescription
 → сохранение версии, исходного файла и provenance
 → поиск совпадающих фактических идентификаторов (если они есть)
 → предложение связать описание с Asset
 → отображение подтверждённых и предполагаемых полей раздельно
```

Поля `DeviceDescription`: `format`, `schema_version`, `manufacturer`, `model`, `id_namespace`, `vendor_id`, `product_id`, `hardware_revision`, `description_source`, `source_hash`, `imported_at`, `device_modules`, `properties`. Это **кандидат**, не утверждённая физическая схема базы. Для одного актива допустимо несколько описаний и версий.

**Достоверность:** `Catalogued` (описание известно), `Observed` (идентификаторы реально получены), `Correlated` (совпали ID с учётом namespace и revision), `Verified` (протокол/оператор подтвердил). Сравнение лишь номера порта или MAC OUI без фактических ID не является точным совпадением.

## 5. Реализация без лишнего усложнения

Первый полезный MVP — **офлайн просмотрщик**: открыть файл, прочитать поля, показать дерево модулей/описаний, сохранить в каталог. Не требуются OPC UA сервер, реализация GSDML-проекта, raw Ethernet доступ или новый сетевой драйвер. Затем можно добавить корреляцию с проектом и наблюдениями; лишь потом — адресные read-only запросы при необходимости.

## 6. Нерешённые вопросы

Какие парсеры первыми; версионирование, локальные каталоги и обновления; как представлять модульные устройства, port/slave, device variants; какие IDs считать достаточными для автоматической корреляции; нужен ли отдельный portable export; способы импорта данных инженерных проектов. **Все решения отложены до отдельного согласования** — [DECISIONS.md](DECISIONS.md).
