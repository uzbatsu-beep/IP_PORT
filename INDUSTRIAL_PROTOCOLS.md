# Industrial protocol reference — initial research 2026-11-26

Machine-readable catalog: [data/industrial_protocols.json](data/industrial_protocols.json), 52 entries. The Knowledge Base view reads this bundled resource offline. It is deliberately **not** the scanner service-name registry or a probe dispatch table.

## Interpretation

- documented_default: vendor documentation or protocol specification gives a port; actual enabled service and direction depend on configuration.
- conditional: an option, firewall rule or conventional port, not guaranteed listening endpoint.
- configurable: application/project decides; no universal port number.
- unverified_port: function documented but exact port/transport unresolved.
- layer2: EtherType rather than TCP/UDP port.
- registered_candidate: registry-level hint, not verified on a specific device.
- research_pending: roadmap coverage only; verify dedicated sources before promoting.

For TCP, connect success indicates an accepting endpoint, not application identity. For UDP, no response cannot distinguish silent service from filtering; use explicit valid application replies only when a separate adapter is approved. MAC OUI identifies an allocation owner, not exact device model. Routers, gateways, NAT and virtual PLCs can obscure identity. Prefer observation and vendor-supported read-only identification; never use arbitrary payloads on cyclic I/O or management services. No new probes, write commands, broadcast, login attempts or scan presets are enabled by this catalog. Apply rate/timeout constraints and validate each adapter on a controlled bench.

## Catalog

| Vendor / family | Transport | Port / EtherType | Function | Evidence |
| --- | --- | --- | --- | --- |
| Weintek / eMT/iE/XE/mTV/iP; cMT/cMT X | TCP | 21 | FTP | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / eMT/iE/XE/mTV/iP; cMT/cMT X | TCP | 2000 | Ethernet pass-through | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / eMT/iE/XE/mTV/iP | TCP | 80, 5800 | VNC web/Java | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / cMT/cMT X (incl. cMT2108X/cMT2078X family) | TCP | 80, 443 | Project transfer / system settings | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / cMT/cMT X (incl. cMT2108X/cMT2078X family) | TCP | 5800 | WebView | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / eMT/iE/XE/mTV/iP; cMT/cMT X | TCP | 5900 | VNC | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / eMT/iE/XE/mTV/iP; cMT/cMT X | TCP | 8000 | HMI communication | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / eMT/iE/XE/mTV/iP | TCP | 8001 | EasyDiagnoser | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / eMT/iE/XE/mTV/iP | TCP | 8005 | EasyPrinter | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / cMT/cMT X (incl. cMT2108X/cMT2078X family) | TCP | 8002, 10463 | cMT Diagnoser | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / eMT/iE/XE/mTV/iP; cMT/cMT X | TCP | 8010 | Inter-family HMI communication | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / eMT/iE/XE/mTV/iP; cMT/cMT X | TCP | 10275 | HMI search | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / eMT/iE/XE/mTV/iP | TCP | 20248 | Project transfer via router | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Weintek / cMT/cMT X (incl. cMT2108X/cMT2078X family) | TCP | 10763 | cMT Viewer / GUI / diagnostics | [documented_default](https://dl.weintek.com/public/Document/FAQ/FAQ_34_TCP_Port_List_eng.pdf) |
| Yaskawa / YRC1000 / YRC1000micro / DX200 / FS100 / DX100 (DS2.07.00-00+) | UDP | 10040, 10041 | YMConnect / High Speed Ethernet Server | [documented_default](https://knowledge.motoman.com/hc/en-us/articles/33634471611154-YMConnect-Frequently-Asked-Questions) |
| Yaskawa / NX100 | Unknown | — | Ethernet Server Function | [unverified_port](https://knowledge.motoman.com/hc/en-us/articles/4408154769047-Remote-Operations-with-Yaskawa-Motoman-Robots) |
| Yaskawa / DX100 / DX200 / FS100 / YRC controllers | TCP | 21 | FTP service | [conditional](https://knowledge.motoman.com/hc/en-us/articles/13086887178391-Using-File-Transfer-Protocol-FTP-with-Yaskawa-Controllers) |
| Yaskawa / DX100 / DX200 / YRC1000 / YRC1000micro | TCP/UDP | 44818, 2222 (UDP only for I/O) | EtherNet/IP option | [conditional](https://knowledge.motoman.com/hc/en-us/articles/4409682141591-EtherNet-IP-Setup-with-a-Omron-NX-NJ-PLC) |
| Hyundai / Hi6 | TCP/UDP | Configurable | HRScript ENet | [configurable](https://hyundai-robotics.gitbook.io/hi6-robot-language/rl-english/external-comm/enet-tcp-udp) |
| Hyundai / Hi5a (documented integration) | UDP | Configurable | Robot communication | [configurable](https://manual2-hd.web.app/docs/en/hi5a/fn/Hi5aPI230628FMEN2/2/4/2-4-3/) |
| Hyundai / Hi6 | HTTP | Unknown | Open API | [unverified_port](https://github.com/hyundai-robotics/hdr_ros2_driver) |
| Siemens / S7-1500; S7 family candidate | TCP | 102 | ISO-on-TCP / S7 communication | [documented_default](https://support.industry.siemens.com/cs/attachments/59192925/s71500_communication_function_manual_en-US_en-US.pdf?download=true) |
| Siemens / S7-1500 | TCP | 80, 443 | CPU web server | [documented_default](https://support.industry.siemens.com/cs/attachments/59192925/s71500_communication_function_manual_en-US_en-US.pdf?download=true) |
| Siemens / PI / PROFINET | Ethernet L2 | 0x8892 | DCP / cyclic IO | [layer2](https://support.industry.siemens.com/cs/attachments/59192925/s71500_communication_function_manual_en-US_en-US.pdf?download=true) |
| PI / IEEE / LLDP | Ethernet L2 | 0x88CC | Link neighbor discovery | [layer2](https://support.industry.siemens.com/cs/attachments/59192925/s71500_communication_function_manual_en-US_en-US.pdf?download=true) |
| Omron / CS1W-EIP21/EIP21S | TCP/UDP | 9600 | FINS | [documented_default](https://www.ia.omron.com/products/family/1994/specification.html) |
| Omron / CS1W-EIP21/EIP21S | TCP/UDP | 44818 | CIP Class 3 / UCMM | [documented_default](https://www.ia.omron.com/products/family/1994/specification.html) |
| Omron / CS1W-EIP21/EIP21S | UDP | 2222 | CIP cyclic tag data links | [documented_default](https://www.ia.omron.com/products/family/1994/specification.html) |
| Omron / CS1W-EIP21/EIP21S | UDP | 161, 162 | SNMP / traps | [documented_default](https://www.ia.omron.com/products/family/1994/specification.html) |
| Omron / NX102 | TCP | 4840 | OPC UA server | [documented_default](https://www.ia.omron.com/products/family/3705/specification.html) |
| CODESYS / Control V3 runtime | UDP | 1740–1743 | Block drivers / discovery | [documented_default](https://content.helpme-codesys.com/en/CODESYS%20Control/_rtsl_start_page.html) |
| CODESYS / Control V3 runtime | TCP | 11740–11743 | Runtime block drivers | [documented_default](https://content.helpme-codesys.com/en/CODESYS%20Control/_rtsl_start_page.html) |
| CODESYS / Gateway | TCP | 1217 | Gateway communication | [documented_default](https://content.helpme-codesys.com/en/CODESYS%20Control/_rtsl_start_page.html) |
| CODESYS / Control V3 runtime | TCP | 8080, 443, 4840 | Web visualization / TLS / OPC UA | [documented_default](https://content.helpme-codesys.com/en/CODESYS%20Control/_rtsl_start_page.html) |
| ОВЕН / ПЛК210 | TCP | 11740 | CODESYS debugging | [documented_default](https://docs.owen.ru/product/plk210/doc/1185/vvedenie?search=codesys) |
| ОВЕН / ПЛК210 | UDP | 1740 | CODESYS debugging | [documented_default](https://docs.owen.ru/product/plk210/doc/1185/vvedenie?search=codesys) |
| ОВЕН / ПЛК210 | TCP/UDP | 502 | Modbus firewall rule | [conditional](https://docs.owen.ru/product/plk210/doc/1185/vvedenie?search=codesys) |
| Beckhoff / TwinCAT ADS | TCP | 48898, 8016 | ADS / Secure ADS | [documented_default](https://infosys.beckhoff.com/content/1033/tc3_ads_intro/115824651.html) |
| Beckhoff / TwinCAT ADS | UDP | 48899 | Broadcast discovery | [documented_default](https://infosys.beckhoff.com/content/1033/tc3_ads_intro/115824651.html) |
| Open protocol / Modbus TCP | TCP | 502 | Modbus TCP | [documented_default](https://www.modbus.org/faq) |
| Open protocol / OPC UA | TCP | 4840 | UA TCP discovery / server | [documented_default](https://reference.opcfoundation.org/specs/OPC-10000-6/7.6) |
| ODVA / EtherNet/IP | UDP | 44818 | ListIdentity | [documented_default](https://www.odva.org/publication_download/cip-security-a-valuable-tool-for-cra-compliance/) |
| Rockwell / Allen-Bradley / EtherNet/IP | TCP/UDP | 44818 | Encapsulation / messaging | [documented_default](https://www.rockwellautomation.com/en-ca/docs/factorytalk-services-platform/6-60/factorytalk-services-platform-help-ditamap/factorytalk-linx-help/config-stand-alone-apps/add-a-driver/ethernet-ppt-advanced-sets.html) |
| Schneider Electric / Modicon M241/M251/M262 | TCP | 502 | Embedded Modbus server | [documented_default](https://product-help.schneider-electric.com/Machine%20Expert/V2.0/en/MBHandLi/MBHandLi/GeneralInformation-7421D521.html) |
| Mitsubishi Electric / MELSEC-L / SLMP / MC | TCP/UDP | Configurable | SLMP / MC protocol | [configurable](https://www.mitsubishielectric.com/fa/products/cnt/plcl/pmerit/network/index.html) |
| Standards / infrastructure / SNMP | UDP | 161 | Agent | [registered_candidate](https://www.iana.org/assignments/service-names-port-numbers) |
| Standards / infrastructure / BACnet/IP | UDP | 47808 | Building automation | [registered_candidate](https://www.iana.org/assignments/service-names-port-numbers) |
| Standards / infrastructure / IEC 60870-5-104 | TCP | 2404 | Telecontrol | [registered_candidate](https://www.iana.org/assignments/service-names-port-numbers) |
| Standards / infrastructure / DNP3 | TCP/UDP | 20000 | Telecontrol | [registered_candidate](https://www.iana.org/assignments/service-names-port-numbers) |
| Standards / infrastructure / MQTT | TCP | 1883, 8883 | Broker / TLS | [registered_candidate](https://www.iana.org/assignments/service-names-port-numbers) |
| Standards / infrastructure / EtherCAT | Ethernet L2 | 0x88A4 | Cyclic fieldbus | [layer2](https://www.ethercat.org/en/technology.html) |
| Standards / infrastructure / PROFIBUS / CANopen / IO-Link / Modbus RTU | Non-IP | — | Fieldbus / serial | [research_pending](https://www.iana.org/assignments/service-names-port-numbers) |

| Universal Robots / CB3 / PolyScope 5 (legacy Realtime interface) | TCP | 30003 | Realtime client | [conditional](https://docs.universal-robots.com/tutorials/communication-protocol-tutorials/realtime-guide.html) |
| Universal Robots / UR controller / RTDE | TCP | 30004 | RTDE data exchange | [documented_default](https://docs.universal-robots.com/tutorials/communication-protocol-tutorials/rtde-guide.html) |
| Universal Robots / CB3 / PolyScope 5 | TCP | 29999, 30001, 30002 | Dashboard / Primary / Secondary interfaces | [conditional](https://docs.universal-robots.com/Universal_Robots_ROS2_Documentation/doc/ur_client_library/doc/setup/ursim_docker.html) |
| Inductive Automation / Ignition 8.1 Gateway | TCP | 8088, 8043, 8060 | HTTP / HTTPS / Gateway Network | [documented_default](https://docs.inductiveautomation.com/docs/8.1/getting-started/quick-start-guide/download-and-install) |

## Research backlog

- NX100 exact Ethernet Server Function port and transport (obtain manual specific to controller/version; YMConnect does not list NX100).
- Hyundai Hi5/Hi5a/Hi6 exact options, firmware and manufacturer discovery/API listener settings. Example HRScript ports are not defaults.
- Weintek search transport beyond published TCP FAQ; test cMT2108X/cMT2078X on bench, direction of service endpoints.
- Siemens S7-300/400/1200 and VIPA/Speed7, Omron CJ/NJ/NX model matrices, OWEN PLK100/160/200 and CODESYS v2.3 differences.
- Additional vendors: KUKA, Kawasaki, ABB, FANUC, Delta, Inovance, WAGO, B&R, Phoenix Contact, Keyence, LS Electric; energy and instrumentation; dedicated protocol documentation and hardware observations.
- Add protocol response validation and identity provenance only in future functional PRs. Do not assign manufacturer confidence from a port pattern alone.


## UI-03.4 candidate presets

Discovery uses a reviewed record-ID allowlist in `src/knowledge_service.rs` instead of blindly converting every catalog row to a port list. Candidate ports are derived from the existing numeric fields using the existing parser, sorted/deduplicated and separated by transport. Vendor/family filtering is exact against catalog labels; combined family labels remain combined. Protocol group membership is explicit. No automatic scanning, identification or port replacement. Conditional entries, unknown/configurable ports, L2, cyclic I/O, trap receivers, and ambiguous directions are excluded. UDP entries are only configuration candidates, not proof that the current generic UDP probe can identify the protocol. Catalog remains 52 records; no new primary-source review is claimed in UI-03.4.


## UI-03.5 integrity and coverage audit

56 records; 52 existing IDs preserved, EtherCAT source corrected to ETG, four new reference-only records (Universal Robots and Ignition). The explicit 26-record candidate allowlist is unchanged. Runtime validation rejects duplicate IDs, empty required fields, non-HTTPS sources, unknown kinds, missing or invalid candidate references and invalid protocol group membership. It does not verify live links or factory settings. Remaining coverage gaps: ABB, KUKA, Kawasaki, FANUC, Delta, Inovance, WAGO, B&R, Phoenix Contact, Keyence, LS Electric, VIPA and older Siemens/Omron/OWEN generations, Hyundai and NX100; model-specific primary sources and bench observations still needed. Catalog coverage is not exhaustive and never implies device identity.
