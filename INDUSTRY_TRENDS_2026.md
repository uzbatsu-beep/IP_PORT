# Промышленные модели данных и стандарты — вектор 2026 года

Дата проверки первоисточников: **2026-11-21**. Этот обзор объясняет, как современные модели оборудования могут повлиять на архитектуру IP_PORT. **Он не является утверждением о поддержке перечисленных протоколов в v0.4.0.**

## 1. OPC UA FX: перспективы унификации на полевом уровне

**Что это:** OPC UA Field eXchange (FX) расширяет подход OPC UA до взаимодействия контроллеров и промышленных устройств, включая стандартизованные AutomationComponents, связи и обмен данными. **Это не просто «OPC UA порт 4840»** и не замена существующих PROFINET/EtherCAT/CIP в любой установке.

**Статус 2026:** опубликован C2C (Controller-to-Controller) набор; OPC Foundation выпустила maintenance release v1.00.04 в июле 2026 (Parts 81/84). C2D (Controller-to-Device, sensors/remote I/O/motion) развивался и демонстрировался; нельзя считать его универсально внедрённым на всех устройствах.

**Польза для IP_PORT:** опираться на идею `Asset`, `AutomationComponent`, `FunctionalEntity`, `NetworkInterface` и явные связи. Позже импортировать/читать доступные OPC UA-модели у устройств, которые их действительно предоставляют. Не строить сейчас собственный стек управления real-time обменом/TSN.

Источники:
- [OPC UA Part 80: FX Overview](https://reference.opcfoundation.org/specs/OPC-10000-80/5.2)
- [OPC UA Part 81: Connecting Devices and Information Model](https://reference.opcfoundation.org/specs/OPC-10000-81)
- [OPC Foundation, September 2026 update](https://opcconnect.opcfoundation.org/2026/09/field-level-communications-corner-september-2026/)

## 2. PA-DIM: единая семантика приборов КИП

**Что это:** Process Automation Device Information Model — стандартная информационная модель, реализуемая средствами OPC UA, позволяющая одинаково представлять параметры полевых приборов независимо от оригинального полевого протокола/производителя.

**Пример:** преобразователь давления работает по HART и доступен через шлюз. PA-DIM позволяет шлюзу/серверу OPC UA представить Vendor, Model, Serial Number, измеряемую величину, диагностические метаданные в общей структуре. **Не означает**, что каждый HART-прибор сам открывает OPC UA endpoint.

**Польза для IP_PORT:** общий профиль «Интеллектуальный датчик/КИП» и стандартизованные свойства в `DeviceIdentity`/`Observation`. В перспективе — сопоставление импортированных EDD/FDI и данных OPC UA по одному физическому AssetID.

Источники:
- [OPC UA for PA-DIM (OPC-30081)](https://reference.opcfoundation.org/specs/OPC-30081/full)
- [FieldComm Group PA-DIM](https://www.fieldcommgroup.org/technologies/pa-dim)

## 3. VDA 5050 v3: обмен сведениями мобильных роботов и fleet management

**Что это:** интерфейс обмена заданиями и статусами между управляющей системой парка и мобильными роботами AMR/AGV. Это **не универсальный диагностический протокол для стационарных шестикоординатных манипуляторов** Yaskawa/KUKA/FANUC.

**Статус 2026:** VDA и VDMA представили версию 3.0 весной 2026, расширив случаи применения для более автономных мобильных роботов. Наличие поддержки зависит от модели и ПО робота/fleet-системы.

**Польза для IP_PORT:** модели `MobileRobot`, `FleetController`, `RobotIdentity`, наблюдаемые статусы и связи в будущей категории робототехники. Позже — разрешённый импорт/анализ данных совместимых fleet-систем без команд перемещения.

Источники:
- [VDA press release 21 April 2026](https://www.vda.de/en/press/press-releases/2026/260421_PM_VDA_5050_EN)
- [VDA 5050 specification and release](https://github.com/VDA5050/VDA5050/releases)

## 4. Asset Administration Shell (AAS): цифровой паспорт актива

**Что это:** стандартизованная структура метаданных о физическом или программном активе; «оболочка» содержит специализированные подмодели: цифровой паспорт/Nameplate, технические данные, документация, характеристики и т.д. AAS — **не сетевой сканер** и не обязательно поток онлайн-телеметрии.

**Пример:** карточка PLC в IP_PORT содержит производителя, модель, серийный номер, положение в цехе, файлы руководств и историю наблюдений. Эти поля можно в будущем сопоставить с AAS Digital Nameplate и другими подмоделями, чтобы передать цифровое описание актива другим системам.

**Польза для IP_PORT:** заранее отделить идентификатор физического `Asset` от IP, MAC и логического `Device`, хранить ссылки на документацию, значения и их источники. Импорт/экспорт AASX оставить перспективным адаптером, не обязательным форматом внутренней SQLite-базы.

Источники:
- [IDTA — specification of AAS metamodel](https://industrialdigitaltwin.org/en/content-hub/aasspecifications/specification-of-the-asset-administration-shell-part-1-metamodel-idta-number-01001)
- [IDTA — official submodel templates](https://industrialdigitaltwin.org/en/content-hub/submodels)
- [IDTA — downloads, including AASX package format](https://industrialdigitaltwin.org/en/content-hub/downloads)

## 5. Общее направление для IP_PORT

| Подход | Что следует перенять сейчас на уровне **архитектуры** | Что отложено |
|---|---|---|
| OPC UA FX | Физические/логические компоненты и связность, стабильные идентификаторы | Реализация промышленного контроллерного real-time стека |
| PA-DIM | Стандартизованные поля КИП, протокол-независимые идентификаторы | Активный универсальный опрос всей полевой сети |
| VDA 5050 | Категория мобильной робототехники, статус/источник наблюдения | Управление парком AGV/AMR |
| AAS | Цифровая карточка оборудования, nameplate, ссылки на документы и связи | Полноценный AAS registry/server и обязательное AASX хранение |

**Итог:** строить в первую очередь компактную, понятную и расширяемую модель устройства. Стандарты дают общий язык описания, но не обязывают IP_PORT реализовывать каждый промышленный протокол. Очерёдность первых модулей **не утверждена** — см. [DECISIONS.md](DECISIONS.md).

Связанные документы: [DEVICE_DESCRIPTIONS.md](DEVICE_DESCRIPTIONS.md), [ARCHITECTURE.md](ARCHITECTURE.md), [ROADMAP.md](ROADMAP.md).
