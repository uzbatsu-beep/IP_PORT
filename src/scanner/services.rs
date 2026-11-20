use std::fs;
use std::path::Path;
use std::sync::{LazyLock, RwLock};

/// Глобальная база соответствия портов и сервисов (срез IANA/Nmap с поддержкой внешних файлов).
static GLOBAL_SERVICES: LazyLock<RwLock<ServicesDb>> = LazyLock::new(|| {
    let mut db = ServicesDb::default_builtin();
    db.auto_discover();
    RwLock::new(db)
});

/// Быстрое определение предполагаемого сервиса по номеру порта.
pub fn service_name(port: u16) -> &'static str {
    GLOBAL_SERVICES
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .get(port)
        .unwrap_or("unknown")
}

/// Загружает внешнюю базу соответствия портов (nmap-services, /etc/services или IANA CSV).
pub fn load_services_file(path: &Path) -> Result<usize, String> {
    GLOBAL_SERVICES
        .write()
        .unwrap_or_else(|p| p.into_inner())
        .load_file(path)
}

/// Сбрасывает базу данных сервисов к встроенному срезу по умолчанию.
pub fn reset_services_to_default() {
    let mut lock = GLOBAL_SERVICES.write().unwrap_or_else(|p| p.into_inner());
    *lock = ServicesDb::default_builtin();
}

/// Возвращает описание текущего источника базы сервисов.
pub fn services_db_info() -> String {
    GLOBAL_SERVICES
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .source_info()
        .to_owned()
}

/// Возвращает общее количество загруженных TCP-сервисов.
#[cfg(test)]
pub fn services_count() -> usize {
    GLOBAL_SERVICES
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .count()
}

/// Таблица соответствия портов (массив на 65 536 портов для мгновенного O(1) поиска без хэширования).
pub struct ServicesDb {
    table: Box<[Option<&'static str>]>, 
    source_description: String,
    total_loaded: usize,
}

impl ServicesDb {
    /// Создаёт пустую таблицу.
    pub fn new() -> Self {
        Self {
            // Allocate the ~1 MiB lookup table directly on the heap.
            // Box::new([None; 65536]) can first build a huge temporary array\n            // on the calling thread's stack (often only 1 MiB on Windows).\n            // Lazy initialization from the Services database UI then risks\n            // a stack overflow and aborts the whole process.\n            table: vec![None; 65_536].into_boxed_slice(),
            source_description: "Empty".to_owned(),
            total_loaded: 0,
        }
    }

    /// Возвращает имя сервиса для порта, если оно определено.
    #[inline]
    pub fn get(&self, port: u16) -> Option<&'static str> {
        self.table[port as usize]
    }

    /// Устанавливает соответствие порта имени сервиса (сохраняя строку в 'static).
    pub fn set(&mut self, port: u16, name: &str) {
        if self.table[port as usize].is_none() {
            self.total_loaded += 1;
        }
        let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
        self.table[port as usize] = Some(leaked);
    }

    /// Описание источника и количества загруженных сервисов.
    pub fn source_info(&self) -> &str {
        &self.source_description
    }

    /// Количество загруженных TCP-сервисов.
    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.total_loaded
    }

    /// Парсит строки из формата `nmap-services`, `/etc/services`, IANA CSV или простых `port: service`.
    pub fn parse_str(&mut self, content: &str) -> usize {
        let mut count = 0;
        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Формат IANA CSV: Service Name,Port Number,Transport Protocol,...
            if line.contains(',') {
                let parts: Vec<&str> = line.split(',').map(str::trim).collect();
                if parts.len() >= 3 {
                    let svc = parts[0];
                    let port_str = parts[1];
                    let proto = parts[2];
                    if proto.eq_ignore_ascii_case("tcp")
                        && let Ok(port) = port_str.parse::<u16>()
                        && port > 0
                        && !svc.is_empty()
                    {
                        self.set(port, svc);
                        count += 1;
                        continue;
                    }
                }
            }

            // Формат nmap-services / /etc/services:
            // <service-name> <port>/<protocol> [aliases/frequency...] [# comment]
            let without_comment = line.split('#').next().unwrap_or("").trim();
            if without_comment.is_empty() {
                continue;
            }

            let tokens: Vec<&str> = without_comment.split_whitespace().collect();
            if tokens.len() >= 2 {
                // Токен 1: port/proto (например, 80/tcp)
                if let Some((port_str, proto)) = tokens[1].split_once('/')
                    && proto.eq_ignore_ascii_case("tcp")
                    && let Ok(port) = port_str.parse::<u16>()
                    && port > 0
                {
                    self.set(port, tokens[0]);
                    count += 1;
                    continue;
                }
                // Либо токен 0: port/proto
                if let Some((port_str, proto)) = tokens[0].split_once('/')
                    && proto.eq_ignore_ascii_case("tcp")
                    && let Ok(port) = port_str.parse::<u16>()
                    && port > 0
                {
                    self.set(port, tokens[1]);
                    count += 1;
                    continue;
                }
            }

            // Формат key-value: 80=http или 80: http
            if let Some((k, v)) = line.split_once('=').or_else(|| line.split_once(':')) {
                let k = k.trim();
                let v = v.trim();
                if let Ok(port) = k.parse::<u16>()
                    && port > 0
                    && !v.is_empty()
                {
                    self.set(port, v);
                    count += 1;
                    continue;
                }
                if let Ok(port) = v.parse::<u16>()
                    && port > 0
                    && !k.is_empty()
                {
                    self.set(port, k);
                    count += 1;
                    continue;
                }
            }
        }
        count
    }

    /// Загружает базу данных сервисов из файла по указанному пути.
    pub fn load_file(&mut self, path: &Path) -> Result<usize, String> {
        let content = fs::read_to_string(path)
            .map_err(|err| format!("Failed to read {}: {err}", path.display()))?;
        let count = self.parse_str(&content);
        if count == 0 {
            return Err(format!(
                "No TCP service definitions found in {}",
                path.display()
            ));
        }
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("external file");
        self.source_description = format!(
            "External: {} ({} TCP services)",
            file_name, self.total_loaded
        );
        Ok(count)
    }

    /// Ищет локальные файлы nmap-services или переменную окружения IP_PORT_SERVICES_PATH.
    pub fn auto_discover(&mut self) {
        if let Ok(env_path) = std::env::var("IP_PORT_SERVICES_PATH") {
            let path = Path::new(&env_path);
            if path.is_file() {
                let _ = self.load_file(path);
                return;
            }
        }

        for candidate in ["nmap-services", "nmap-services.txt", "services.txt"] {
            let path = Path::new(candidate);
            if path.is_file() {
                let _ = self.load_file(path);
                return;
            }
        }
    }

    /// Создаёт базу с дефолтным встроенным набором популярных сервисов IANA/Nmap.
    pub fn default_builtin() -> Self {
        let mut db = Self::new();
        // Встроенный срез стандартных портов IANA и популярных сервисов Nmap
        const BUILTIN_PORTS: &[(u16, &str)] = &[
            (7, "echo"),
            (9, "discard"),
            (13, "daytime"),
            (17, "qotd"),
            (19, "chargen"),
            (20, "ftp-data"),
            (21, "ftp"),
            (22, "ssh"),
            (23, "telnet"),
            (25, "smtp"),
            (42, "nameserver"),
            (43, "whois"),
            (53, "dns"),
            (67, "dhcps"),
            (68, "dhcpc"),
            (69, "tftp"),
            (70, "gopher"),
            (79, "finger"),
            (80, "http"),
            (88, "kerberos"),
            (102, "iso-tsap"),
            (110, "pop3"),
            (111, "rpcbind"),
            (113, "ident"),
            (119, "nntp"),
            (123, "ntp"),
            (135, "msrpc"),
            (139, "netbios"),
            (143, "imap"),
            (161, "snmp"),
            (162, "snmp-trap"),
            (179, "bgp"),
            (389, "ldap"),
            (443, "https"),
            (445, "smb"),
            (465, "smtps"),
            (500, "isakmp"),
            (514, "syslog"),
            (515, "printer"),
            (548, "afp"),
            (554, "rtsp"),
            (587, "smtp-sub"),
            (631, "ipp"),
            (636, "ldaps"),
            (873, "rsync"),
            (993, "imaps"),
            (995, "pop3s"),
            (1080, "socks"),
            (1194, "openvpn"),
            (1433, "mssql"),
            (1521, "oracle"),
            (1723, "pptp"),
            (1812, "radius"),
            (1883, "mqtt"),
            (1900, "ssdp"),
            (2049, "nfs"),
            (2181, "zookeeper"),
            (2375, "docker"),
            (2376, "docker-tls"),
            (2379, "etcd-client"),
            (2380, "etcd-server"),
            (3000, "grafana"),
            (3128, "squid"),
            (3268, "ms-gc"),
            (3269, "ms-gc-ssl"),
            (3306, "mysql"),
            (3389, "rdp"),
            (3702, "ws-discovery"),
            (4444, "metasploit"),
            (5000, "http-alt"),
            (5060, "sip"),
            (5061, "sips"),
            (5222, "xmpp-client"),
            (5353, "mdns"),
            (5432, "postgres"),
            (5601, "kibana"),
            (5671, "amqps"),
            (5672, "amqp"),
            (5900, "vnc"),
            (5984, "couchdb"),
            (5985, "winrm-http"),
            (5986, "winrm-https"),
            (6000, "x11"),
            (6379, "redis"),
            (6443, "k8s-api"),
            (6667, "irc"),
            (7000, "cassandra"),
            (7001, "weblogic"),
            (7077, "spark"),
            (8000, "http-alt"),
            (8008, "http-alt"),
            (8080, "http-proxy"),
            (8081, "http-alt"),
            (8123, "clickhouse"),
            (8200, "trivnet1"),
            (8443, "https-alt"),
            (8500, "consul"),
            (8883, "mqtt-tls"),
            (8888, "http-alt"),
            (9000, "php-fpm"),
            (9001, "minio-console"),
            (9042, "cassandra"),
            (9090, "prometheus"),
            (9092, "kafka"),
            (9100, "jetdirect"),
            (9200, "elasticsearch"),
            (9300, "elasticsearch"),
            (9418, "git"),
            (10000, "webmin"),
            (10050, "zabbix-agent"),
            (10051, "zabbix-trapper"),
            (11211, "memcached"),
            (15672, "rabbitmq"),
            (25565, "minecraft"),
            (27017, "mongodb"),
            (32400, "plex"),
            (50000, "sap"),
        ];
        for &(port, name) in BUILTIN_PORTS {
            db.set(port, name);
        }
        db.source_description = format!("Built-in Nmap/IANA ({} TCP services)", db.total_loaded);
        db
    }
}

impl Default for ServicesDb {
    fn default() -> Self {
        Self::default_builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services_database_can_initialize_on_a_small_ui_thread_stack() {
        // This test catches large temporary stack allocations when creating
        // the 65_536-slot service database. Windows GUI main threads typically
        // have a much smaller stack than Rust's test harness threads.
        std::thread::Builder::new()
            .name("services-ui-small-stack".to_owned())
            .stack_size(256 * 1024)
            .spawn(|| {
                let mut db = ServicesDb::new();
                assert!(db.get(443).is_none());
                db.set(443, "https");
                assert_eq!(db.get(443), Some("https"));

                let builtin = ServicesDb::default_builtin();
                assert_eq!(builtin.get(22), Some("ssh"));
                assert!(builtin.source_info().contains("Built-in"));
            })
            .expect("could not spawn small-stack test thread")
            .join()
            .expect("service database initialization on small stack failed");
    }

    #[test]
    fn parses_nmap_services_format() {
        let mut db = ServicesDb::new();
        let sample_nmap = r#"
# Nmap services sample
tcpmux	1/tcp	0.001995	# TCP Port Service Multiplexer
echo	7/tcp	0.004860
discard	9/udp	0.028290	# UDP should be ignored
ssh	22/tcp	0.182295	# Secure Shell
http	80/tcp	0.484180	# World Wide Web
custom-app	8088/tcp	0.000100
"#;
        let count = db.parse_str(sample_nmap);
        assert_eq!(count, 5);
        assert_eq!(db.get(1), Some("tcpmux"));
        assert_eq!(db.get(7), Some("echo"));
        assert_eq!(db.get(9), None); // UDP was skipped
        assert_eq!(db.get(22), Some("ssh"));
        assert_eq!(db.get(80), Some("http"));
        assert_eq!(db.get(8088), Some("custom-app"));
        assert_eq!(db.get(9999), None);
    }

    #[test]
    fn parses_iana_csv_format() {
        let mut db = ServicesDb::new();
        let sample_iana = r#"
Service Name,Port Number,Transport Protocol,Description
http,80,tcp,World Wide Web HTTP
https,443,tcp,HTTP over TLS/SSL
domain,53,udp,Domain Name System
"#;
        let count = db.parse_str(sample_iana);
        assert_eq!(count, 2);
        assert_eq!(db.get(80), Some("http"));
        assert_eq!(db.get(443), Some("https"));
        assert_eq!(db.get(53), None); // UDP was skipped
    }

    #[test]
    fn parses_etc_services_format() {
        let mut db = ServicesDb::new();
        let sample_etc = r#"
# Windows / Linux /etc/services format
ftp             21/tcp
ssh             22/tcp    # SSH Remote Login
telnet          23/tcp
smtp            25/tcp    mail
http            80/tcp    www www-http
"#;
        let count = db.parse_str(sample_etc);
        assert_eq!(count, 5);
        assert_eq!(db.get(21), Some("ftp"));
        assert_eq!(db.get(22), Some("ssh"));
        assert_eq!(db.get(25), Some("smtp"));
        assert_eq!(db.get(80), Some("http"));
    }

    #[test]
    fn loads_from_file_and_overrides_service() {
        let dir = std::env::temp_dir().join("ip_port_services_test");
        let _ = fs::create_dir_all(&dir);
        let file_path = dir.join("custom-services.txt");

        fs::write(
            &file_path,
            "my-custom-service\t8080/tcp\t# custom service\nanother-svc\t9999/tcp\n",
        )
        .unwrap();

        let mut db = ServicesDb::default_builtin();
        let loaded = db.load_file(&file_path).unwrap();
        assert_eq!(loaded, 2);
        assert_eq!(db.get(8080), Some("my-custom-service"));
        assert_eq!(db.get(9999), Some("another-svc"));
        // Built-in ports still exist
        assert_eq!(db.get(22), Some("ssh"));
        assert_eq!(db.get(80), Some("http"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn global_service_name_resolves_builtin_and_updates() {
        assert_eq!(service_name(22), "ssh");
        assert_eq!(service_name(80), "http");
        assert_eq!(service_name(1900), "ssdp");
        assert_eq!(service_name(3702), "ws-discovery");
        assert_eq!(service_name(5353), "mdns");
        assert_eq!(service_name(8200), "trivnet1");
        assert!(services_count() >= 100);

        let dir = std::env::temp_dir().join("ip_port_global_services_test");
        let _ = fs::create_dir_all(&dir);
        let file_path = dir.join("test-services.txt");
        fs::write(&file_path, "test-override\t22/tcp\n").unwrap();

        load_services_file(&file_path).unwrap();
        assert_eq!(service_name(22), "test-override");

        reset_services_to_default();
        assert_eq!(service_name(22), "ssh");

        let _ = fs::remove_dir_all(&dir);
    }
}
