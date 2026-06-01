# ToS — Translation of Service
## Documento Técnico Completo v0.1
> OpceanAI — Agua (Arquitecto)

---

## Tabla de Contenidos

1. [¿Qué es ToS?](#qué-es-tos)
2. [Por qué existe ToS](#por-qué-existe-tos)
3. [Problemas que resuelve](#problemas-que-resuelve)
4. [Arquitectura General](#arquitectura-general)
5. [ToS-SDL — Schema Definition Language](#tos-sdl--schema-definition-language)
6. [Sistema de Tipos Universal](#sistema-de-tipos-universal)
7. [Adaptadores](#adaptadores)
8. [Protocolo P2P](#protocolo-p2p)
9. [Wire Format](#wire-format)
10. [Criptografía y Seguridad](#criptografía-y-seguridad)
11. [Modos de Operación](#modos-de-operación)
12. [Topologías Multi-Nodo](#topologías-multi-nodo)
13. [CLI](#cli)
14. [Estructura del Proyecto](#estructura-del-proyecto)
15. [Decisiones de Diseño](#decisiones-de-diseño)
16. [Roadmap](#roadmap)

---

## ¿Qué es ToS?

ToS (Translation of Service) es un **protocolo abierto P2P** para mover y sincronizar datos estructurados entre cualquier fuente y cualquier destino, en tiempo real, sin broker central, sin infraestructura extra.

El nombre tiene doble significado intencional:

- **Translation of Service** — traduce schemas entre sistemas de bases de datos heterogéneos
- **Tos** — como la tos real: contagia de nodo a nodo, se propaga sola por la red

```
Node A ──tos──→ Node B ──tos──→ Node C
(MySQL)        (PostgreSQL)      (Redis)
```

ToS no es una herramienta. No es un SaaS. Es un **protocolo** — como HTTP, como TCP. Cualquiera puede implementarlo. Nadie lo controla.

---

## Por qué existe ToS

### El gap real

Hoy existen tres tipos de soluciones para mover datos entre bases de datos:

**1. ETL centralizado (Fivetran, Airbyte)**
- Tus datos pasan por sus servidores
- $600+/mes por integración
- Batch, no tiempo real
- Cerrado, dependencia permanente

**2. Streaming centralizado (Kafka + Debezium)**
- Requiere un broker central (Kafka)
- $50k+/año en infraestructura
- 100ms+ de latencia
- JVM, complejidad operacional brutal
- No traduce schemas automáticamente

**3. Soluciones manuales (triggers + NOTIFY + listeners)**
- Código custom frágil por cada relación
- Solo funciona entre el mismo tipo de DB
- Si falla una app, los datos se dessincronizan
- Imposible escalar a múltiples destinos

**Lo que ninguno hace:**
```
P2P directo
+ Schema traducido automáticamente
+ Tiempo real (streaming, no batch)
+ Multi-nodo (fan-out, merge, chains)
+ Heterogéneo (cualquier DB → cualquier DB)
+ Abierto como protocolo
+ Sin broker central
```

### Por qué Apache nunca lo hizo

Apache Kafka domina el mercado de data pipelines desde 2011. Kafka resolvió el problema centralizando — funciona perfecto para pipelines de escala masiva, pero introduce un broker obligatorio. El mundo construyó sobre Kafka y nadie tuvo incentivo para reinventar el modelo.

Los fabricantes de bases de datos resuelven replicación de forma silos: PostgreSQL replica a PostgreSQL, MySQL a MySQL. Nadie conectó los puntos entre sistemas heterogéneos en un protocolo P2P.

El gap existe porque el mercado asumió que Kafka era suficiente. Para OpceanAI, no lo es.

---

## Problemas que resuelve

### Pitch 1 — Migración sin dolor
Mover datos estructurados de cualquier fuente con schema a cualquier destino con schema. Un comando, schema traducido automáticamente, datos migrados en paralelo.

```bash
tos push --from mysql://legacy/db --to postgres://new/db
# ToS infiere schemas, mapea tipos, mueve datos
# Sin scripts manuales. Sin sorpresas.
```

### Pitch 2 — Sync real-time multi-nodo sin broker
Sincronizar múltiples bases de datos en tiempo real, con topologías arbitrarias, sin necesidad de Kafka, sin infraestructura extra.

```bash
tos sync --from postgres://main \
         --to redis://cache \
         --to json:///backups/data.json \
         --watch
# Cambio en Postgres → Redis y JSON se actualizan automáticamente
```

### Pitch 3 — Cache invalidation automática
Cache invalidation es "uno de los dos problemas más difíciles en Computer Science". ToS lo resuelve: cualquier cambio en la fuente de verdad (PostgreSQL) se propaga automáticamente al cache (Redis) en tiempo real, P2P, sin triggers manuales, sin retry logic, sin stale data bugs.

```
Sin ToS:
  PostgreSQL NOTIFY → listener process → Redis DEL → stale window → bugs

Con ToS:
  PostgreSQL → [ToS Protocol] → Redis
  (automático, garantizado, tipado)
```

---

## Arquitectura General

ToS se organiza en 5 capas:

```
┌─────────────────────────────────────────────────────────┐
│                      CLI / SDK                          │  Capa 5
├─────────────────────────────────────────────────────────┤
│              Adaptadores (por cada DB)                  │  Capa 4
│   MySQL | PostgreSQL | MongoDB | Redis | JSON | YAML    │
├─────────────────────────────────────────────────────────┤
│              Protocolo P2P (ToS-Proto)                  │  Capa 3
│     Handshake | Stream | ACK | Topología multi-nodo     │
├─────────────────────────────────────────────────────────┤
│              Wire Format (ToS-Wire)                     │  Capa 2
│          Binario | BLAKE3 | Cifrado opcional            │
├─────────────────────────────────────────────────────────┤
│    SDL + Sistema de Tipos Universal (ToS-Core)          │  Capa 1
│       Schema Definition | Type system | Resolución      │
└─────────────────────────────────────────────────────────┘
```

Cada capa es independiente. Puedes usar solo el SDL sin el protocolo. Puedes implementar un adapter sin cambiar el protocolo. El protocolo no sabe qué datos mueve — solo sabe que son bytes que corresponden a un schema.

---

## ToS-SDL — Schema Definition Language

### Por qué SDL propio y no JSON Schema / Avro / Protobuf

- **JSON Schema**: diseñado para validación, no para traducción entre DBs. No tiene concepto de primary key, índices, relaciones.
- **Avro**: ligado al ecosistema Kafka. Schema registry centralizado. No encaja con P2P.
- **Protobuf**: diseñado para RPC, no para representar schemas de DBs. No tiene NULL semántics nativas.
- **TOML propio**: legible por humanos, declarativo, expresa exactamente lo que ToS necesita y nada más.

### Formato SDL

```toml
# Ejemplo: tabla users

[schema.users]
id         = { type = "uuid", primary = true }
name       = { type = "text", max = 255, nullable = false }
email      = { type = "text", unique = true }
age        = { type = "uint16" }
metadata   = { type = "map<text, any>" }   # → JSONB en PG, Document en Mongo
active     = { type = "bool", default = true }
created_at = { type = "timestamp(tz)", default = "now" }
deleted_at = { type = "timestamp(tz)", nullable = true }

[schema.users.indexes]
email_idx  = { fields = ["email"], unique = true }
active_idx = { fields = ["active", "created_at"] }

[schema.users.relations]
posts = { type = "has_many", schema = "posts", foreign_key = "user_id" }
```

### Por qué TOML y no YAML o JSON

- TOML tiene tipos nativos (enteros, flotantes, fechas, booleanos) — YAML también pero es ambiguo y YAML es conocido por errores silenciosos
- TOML es más legible que JSON para estructuras con metadatos por campo
- TOML es determinístico — sin sorpresas de parsing
- JSON no tiene comentarios

### Schema Inference

Cuando la fuente es JSON, YAML o TXT, ToS infiere el schema automáticamente:

```bash
tos schema infer --from json:///data/users.json
# Output: schema inferred → schema.tos
```

Reglas de inferencia:

```
"123"        → intenta uint32, si overflows → int64, si falla → text
"3.14"       → float64
"true/false" → bool
"2026-01-01" → date
"uuid-v4"    → uuid
resto        → text
null         → optional<inferred_type>
{}           → map<text, any>
[]           → array<inferred_type>
```

Si el usuario quiere control explícito, define el schema manualmente antes de mover. La inferencia es best-effort, nunca destruye datos.

---

## Sistema de Tipos Universal

### Por qué un type system propio

Cada base de datos tiene su propio sistema de tipos:

- PostgreSQL: `INTEGER`, `BIGINT`, `NUMERIC`, `TEXT`, `JSONB`, `UUID`, `TIMESTAMPTZ`, `ARRAY`, tipos geométricos...
- MySQL: `INT`, `BIGINT`, `DECIMAL`, `VARCHAR`, `JSON`, `DATETIME`...
- MongoDB: BSON types (`ObjectId`, `Date`, `Decimal128`, `Binary`...)
- Redis: strings, hashes, lists, sets, sorted sets, streams
- JSON: number (float64), string, boolean, null, array, object

Sin un tipo universal intermediario, cada adapter tendría que saber traducir hacia todos los demás O(n²) combinaciones. Con el type system universal, cada adapter solo traduce hacia/desde el tipo ToS: O(n).

### Tipos Primitivos

```
Numéricos:
  bool
  int8   / int16  / int32  / int64
  uint8  / uint16 / uint32 / uint64
  float32 / float64
  decimal(precision, scale)     → para dinero, no usar float

Texto / Binario:
  text(max?)                    → max es hint, no hard limit
  bytes(max?)                   → datos binarios arbitrarios

Identidad:
  uuid                          → UUID v4 por defecto

Tiempo:
  timestamp(tz?)                → con o sin timezone
  date                          → solo fecha, sin hora
  time                          → solo hora, sin fecha
  duration                      → intervalo de tiempo

Especiales:
  any                           → sin tipo definido (escape hatch)
```

### Tipos Compuestos

```
optional<T>              → nullable en SQL, undefined en JSON
array<T>                 → lista ordenada de T
map<K, V>                → clave-valor (JSONB, Document, Hash)
enum(val1, val2, ...)    → conjunto finito de valores
union<T1, T2>            → cuando el tipo varía (schema evolution)
```

### Cada Adapter Implementa

```rust
trait TosAdapter {
    fn to_native(tos_type: &TosType) -> NativeType;
    fn from_native(native: &NativeType) -> Result<TosType, TypeError>;
    fn read_schema(conn: &Connection) -> Result<TosSchema>;
    fn write_schema(conn: &Connection, schema: &TosSchema) -> Result<()>;
    fn read_records(conn: &Connection, table: &str) -> RecordStream;
    fn write_records(conn: &Connection, table: &str, stream: RecordStream) -> Result<()>;
    fn watch(conn: &Connection, table: &str) -> ChangeStream;  // para sync
}
```

### Resolución de Conflictos de Tipos

Cuando los tipos no mapean 1:1, tres niveles en orden de preferencia:

**Nivel 1: Lossless** — Mapeo sin pérdida de información
```
int32 MySQL          → integer PostgreSQL       ✓ silencioso
VARCHAR(255) MySQL   → text PostgreSQL          ✓ silencioso
```

**Nivel 2: Lossy con warning** — Hay pérdida potencial, se avisa
```
JSONB PostgreSQL     → text MySQL               ⚠ serializa como JSON string
float64              → decimal(10,2)            ⚠ redondeo posible
TIMESTAMPTZ          → DATETIME (sin tz)        ⚠ pierde timezone info
```

**Nivel 3: Rechazo** — El mapeo corrompería datos
```
array<uuid> → int32                             ✗ imposible, falla con reporte
```

**Nivel 4: Custom resolver** — El usuario define la transformación
```toml
[resolve.users.metadata]
from = "jsonb"
to   = "text"
fn   = "json_stringify"   # función built-in o custom
```

---

## Adaptadores

### Adaptadores planeados (v1.0)

| Adapter | Fuente | Destino | Watch | Notas |
|---------|--------|---------|-------|-------|
| PostgreSQL | ✓ | ✓ | ✓ | Via logical replication / LISTEN |
| MySQL / MariaDB | ✓ | ✓ | ✓ | Via binlog |
| MongoDB | ✓ | ✓ | ✓ | Via change streams |
| Redis | ✓ | ✓ | ✓ | Via keyspace notifications |
| SQLite | ✓ | ✓ | ✗ | No tiene CDC nativo, polling |
| JSON | ✓ | ✓ | ✓ | Via inotify (file watcher) |
| YAML | ✓ | ✓ | ✓ | Via inotify |
| TXT / CSV | ✓ | ✓ | ✓ | Con config de delimiter |

### Adaptadores planeados (post-v1.0)

- Apache Parquet (columnar, analytics)
- Apache Arrow IPC (zero-copy, in-memory)
- ClickHouse (analytics DB)
- Cassandra / ScyllaDB (column-family)
- REST API (response JSON)
- S3 / GCS (objetos JSON/CSV/Parquet)

### Por qué TXT tiene soporte nativo

Muchos sistemas legados, scripts de exportación, y "bases de datos caseras" viven en TXT/CSV. Si ToS solo soporta DBs reales, excluye a una fracción enorme de usuarios reales. El soporte TXT reduce la barrera de entrada a cero: si tienes datos, ToS los mueve.

### Configuración TXT

```toml
[source]
type       = "txt"
path       = "/data/users.csv"
delimiter  = ","       # coma, tab, pipe, espacio
has_header = true
encoding   = "utf-8"
quote_char = "\""
null_str   = "NULL"    # qué string representa NULL
```

---

## Protocolo P2P

### Por qué QUIC y no TCP

- **QUIC** tiene TLS 1.3 built-in — seguridad sin configuración extra
- **Multiplexing nativo** — múltiples streams sobre una conexión sin head-of-line blocking
- **NAT traversal** — los nodos pueden conectarse sin configurar firewalls
- **Reconexión rápida** — 0-RTT handshake para reconexiones
- **TCP** no tiene ninguna de estas propiedades por defecto

Crate en Rust: `quinn` (implementación de QUIC pura en Rust)

### Estados del Protocolo

```
DISCONNECTED
    │
    ▼ connect()
HANDSHAKING
    │
    ├─ HELLO enviado
    │
    ▼ HELLO_ACK recibido
READY
    │
    ├──────────────────────────────┐
    ▼ push/sync iniciado           │ watch activo
SCHEMA_NEGOTIATION                 │
    │                              │
    ▼ SCHEMA_CONFIRM               │
STREAMING                          │
    │                              │
    ▼ STREAM_END                   │
DONE ──────────────────────────────┘
```

### Mensajes del Protocolo

```rust
// Handshake
HELLO {
    version:    u8,           // versión del protocolo ToS
    node_id:    [u8; 32],     // BLAKE3(public_key)
    public_key: [u8; 32],     // Ed25519 public key
    encrypt:    bool,         // ¿usar cifrado?
    hash_algo:  HashAlgo,     // blake3 | sha256
    caps:       Vec<String>,  // ["postgres", "redis", "json", ...]
}

HELLO_ACK {
    version:    u8,
    node_id:    [u8; 32],
    public_key: [u8; 32],
    x25519_pub: Option<[u8; 32]>,  // key exchange si encrypt=true
    caps:       Vec<String>,
}

// Negociación de Schema
SCHEMA_OFFER {
    sdl:           Vec<u8>,   // contenido SDL en TOML
    schema_hash:   [u8; 32],  // BLAKE3(sdl)
    signature:     [u8; 64],  // Ed25519.sign(schema_hash)
}

SCHEMA_DIFF {
    resolutions:   Vec<TypeResolution>,  // cómo resolver conflictos
    accepted:      bool,
    reason:        Option<String>,       // si rejected
}

SCHEMA_CONFIRM {}   // el sender acepta las resoluciones

// Streaming de Datos
STREAM_START {
    session_id: [u8; 32],
    table:      String,
    mode:       StreamMode,   // Full | Incremental | Watch
    batch_size: u32,
}

BATCH {
    batch_id:   u32,
    records:    Vec<u8>,      // Wire format (ver sección Wire)
    batch_hash: [u8; 32],     // BLAKE3(records_plain)
    signature:  [u8; 64],     // Ed25519.sign(batch_hash)
    count:      u32,
}

ACK    { batch_id: u32 }
NACK   { batch_id: u32, reason: String }

STREAM_END {
    session_id:    [u8; 32],
    total_records: u64,
    duration_ms:   u64,
}

DONE {
    session_id: [u8; 32],
    stats:      SessionStats,
}

// Watch (sync tiempo real)
CHANGE {
    table:     String,
    op:        ChangeOp,    // Insert | Update | Delete
    before:    Option<Vec<u8>>,  // estado anterior (Update/Delete)
    after:     Option<Vec<u8>>,  // estado nuevo (Insert/Update)
    change_id: [u8; 32],
    timestamp: u64,
}

CHANGE_ACK { change_id: [u8; 32] }
```

### Handshake Completo

```
Node A (MySQL)                           Node B (PostgreSQL)
     │                                          │
     │──── HELLO (v1, node_id_A, pk_A) ───────→│
     │←─── HELLO_ACK (v1, node_id_B, pk_B) ────│
     │         [si encrypt=true: x25519_pubs]   │
     │                                          │
     │  [derivar session_key via X25519 ECDH]   │
     │                                          │
     │──── SCHEMA_OFFER (sdl, hash, sig) ──────→│
     │         [B verifica sig con pk_A]        │
     │←─── SCHEMA_DIFF (resoluciones) ──────────│
     │──── SCHEMA_CONFIRM ─────────────────────→│
     │                                          │
     │──── STREAM_START (session_id, table) ───→│
     │──── BATCH(0, records, hash, sig) ───────→│
     │←─── ACK(0) ───────────────────────────── │
     │──── BATCH(1, records, hash, sig) ───────→│
     │←─── ACK(1) ──────────────────────────────│
     │         [si NACK: retransmitir]           │
     │──── STREAM_END ─────────────────────────→│
     │←─── DONE(stats) ─────────────────────────│
```

---

## Wire Format

### Por qué formato binario propio y no JSON/CSV

- JSON: verbose, parsing lento, no tipado
- CSV: ambiguo, problemas con delimitadores en datos, sin tipos
- Parquet: columnar, demasiado pesado para streaming P2P
- MessagePack: buena opción, pero no zero-copy
- **Arrow IPC**: columnar, zero-copy, el mejor para volúmenes grandes

### Wire Format para Batches

```
Batch header (fijo, 20 bytes):
  [schema_hash: 8 bytes]    // primeros 8 bytes del BLAKE3 del SDL
  [batch_id:    4 bytes]    // u32, monotónico
  [record_count: 4 bytes]   // u32
  [format:      1 byte]     // 0x01 = Arrow IPC, 0x02 = MessagePack
  [flags:       3 bytes]    // reserved

Batch body:
  [records: variable]       // Arrow IPC o MessagePack según format byte
```

**Arrow IPC** cuando:
- Batch > 10,000 registros
- Columnas numéricas (zero-copy es enorme ganancia)
- Analytics downstream (ClickHouse, Parquet)

**MessagePack** cuando:
- Batch < 10,000 registros
- Schema con muchos tipos heterogéneos
- Nodos con poca memoria (ARM, mobile)

### Change Record (para Watch mode)

```
[change_id:  16 bytes]   // UUID v4
[timestamp:   8 bytes]   // Unix nanoseconds
[op:          1 byte]    // 0x01=Insert 0x02=Update 0x03=Delete
[table_len:   2 bytes]   // longitud del nombre de tabla
[table:   variable]
[before:  variable]      // serializado con Wire Format (puede ser null)
[after:   variable]      // serializado con Wire Format (puede ser null)
```

---

## Criptografía y Seguridad

### Decisiones

| Componente | Algoritmo | Por qué |
|------------|-----------|---------|
| Hashing integridad | BLAKE3 | Más rápido que SHA-256, especialmente en ARM. No tiene vulnerabilidades conocidas. |
| Firma schemas/batches | Ed25519 | Firma pequeña (64 bytes), verificación rápida, seguro contra timing attacks |
| Key exchange | X25519 (ECDH) | Perfecto para ECDH efímero. Rápido en ARM sin hardware AES |
| Cifrado simétrico | ChaCha20-Poly1305 | Más rápido que AES-GCM en ARM sin aceleración hardware. AEAD (authenticated encryption) |
| Node identity | BLAKE3(Ed25519_pubkey) | Identidad derivada de la clave, no hay CA ni PKI |

### Por qué no mTLS

mTLS requiere PKI, certificados, CAs, renovación. Para un protocolo P2P que debe funcionar en un Redmi 12 y en un VPS de $5, mTLS es overhead innecesario. El modelo de identidad basado en keypairs (estilo libp2p/WireGuard) es más simple y más apropiado para P2P.

### Por qué cifrado es opcional

En muchos escenarios B2B o internos los datos ya viajan por VPN o red privada. Forzar cifrado añade CPU overhead sin beneficio de seguridad real. La integridad (BLAKE3 + firma Ed25519) es siempre obligatoria. El cifrado es un flag en el handshake — la decisión es del operador.

### Flujo de Seguridad

```
1. Cada nodo genera keypair Ed25519 en primer arranque
   keypair → guardado en ~/.tos/identity

2. node_id = BLAKE3(public_key)
   Identidad derivada, no hay registro central

3. Handshake HELLO incluye public_key
   El receptor puede verificar node_id

4. Si encrypt=true:
   Cada parte genera keypair X25519 efímero
   session_key = X25519(my_ephemeral_priv, their_ephemeral_pub)
   Todo el tráfico cifrado con ChaCha20-Poly1305(session_key)

5. SCHEMA_OFFER lleva Ed25519.sign(BLAKE3(sdl))
   El receptor verifica con public_key del HELLO
   Si la firma falla → SCHEMA_DIFF { accepted: false }

6. Cada BATCH lleva Ed25519.sign(BLAKE3(records_plain))
   El receptor verifica antes de escribir
   Si falla → NACK, el sender retransmite
```

### Crates Rust

```toml
[dependencies]
ed25519-dalek    = "2"     # keypair, sign, verify
x25519-dalek     = "2"     # ECDH key exchange
chacha20poly1305 = "0.10"  # cifrado AEAD
blake3           = "1"     # hashing ultrarrápido
quinn            = "0.11"  # QUIC protocol
```

---

## Modos de Operación

### Modo Push (Migración)

Mueve datos una vez, de fuente a destino. Ideal para migraciones, backups, inicialización de un nuevo sistema.

```bash
tos push --from postgres://user:pass@host/db \
         --to   mysql://user:pass@host/db \
         --table users \
         --batch-size 5000

# Con schema explícito
tos push --from mysql://legacy/db \
         --to   postgres://new/db \
         --schema schema.tos
```

**Comportamiento:**
1. Infiere o carga SDL
2. Negocia schema con destino
3. Mueve datos en batches paralelos
4. Verifica integridad por batch
5. Report final con stats

### Modo Sync (Watch)

Sincroniza en tiempo real. Una vez establecida la conexión, cualquier cambio en la fuente se propaga automáticamente al destino.

```bash
tos sync --from postgres://db \
         --to   redis://cache \
         --to   json:///backup/data.json \
         --watch \
         --table users \
         --table products
```

**Comportamiento:**
1. Hace push inicial (estado completo)
2. Activa CDC/watch en la fuente
3. Cada cambio (INSERT/UPDATE/DELETE) genera un CHANGE message
4. El destino aplica el cambio en tiempo real
5. ACK garantiza entrega

### Modo Schema

Operaciones sobre schemas sin mover datos.

```bash
# Leer schema desde DB
tos schema pull postgres://host/db > schema.tos

# Aplicar schema en destino (sin datos)
tos schema push schema.tos --to mysql://host/db

# Inferir schema desde archivo
tos schema infer --from json:///data.json > schema.tos

# Comparar dos schemas
tos schema diff schema_v1.tos schema_v2.tos

# Validar un schema
tos schema validate schema.tos
```

---

## Topologías Multi-Nodo

### Fan-out (1 fuente → N destinos)

```
PostgreSQL ──→ [ToS] ──┬──→ Redis (cache)
                       ├──→ JSON  (backup)
                       └──→ ClickHouse (analytics)
```

```bash
tos sync --from postgres://db \
         --to redis://cache \
         --to json:///backup/export.json \
         --to clickhouse://analytics/db \
         --watch
```

### Merge (N fuentes → 1 destino)

```
MySQL A ──┐
           ├──→ [ToS] ──→ PostgreSQL (consolidado)
MySQL B ──┘
```

```bash
tos sync --from mysql://shard-a/db \
         --from mysql://shard-b/db \
         --to   postgres://main/db \
         --watch
```

### Chain (A → B → C)

```
PostgreSQL ──→ Redis ──→ JSON
```

En chains, cada nodo actúa como fuente para el siguiente. ToS maneja esto con sesiones enlazadas — un DONE en A dispara STREAM_START en B.

### Mesh (arbitrario)

Cualquier topología con múltiples fuentes y destinos. ToS no impone límite de nodos. El usuario define el grafo en un archivo de configuración:

```toml
# tos-topology.toml
[[node]]
id   = "pg-main"
conn = "postgres://host-a/db"
role = "source"

[[node]]
id   = "redis-cache"
conn = "redis://host-b"
role = "destination"

[[node]]
id   = "json-backup"
conn = "json:///backups/data.json"
role = "destination"

[[edge]]
from = "pg-main"
to   = ["redis-cache", "json-backup"]
mode = "sync"
```

```bash
tos topology --file tos-topology.toml --start
```

---

## CLI

### Comandos Principales

```
tos push        Mueve datos una vez (migración)
tos sync        Sincroniza en tiempo real (watch)
tos schema      Operaciones sobre schemas
tos topology    Gestión de topologías multi-nodo
tos node        Gestión de nodo ToS (daemon)
tos status      Estado de sesiones activas
tos log         Logs de transferencias
```

### Referencia Completa

```bash
# tos push
tos push
  --from <uri>            URI de fuente  (postgres://, mysql://, json://, ...)
  --to   <uri>            URI de destino (repetible para múltiples destinos)
  --table <name>          Tabla/colección (repetible, default: todas)
  --schema <file>         SDL explícito (si no se infiere)
  --batch-size <n>        Registros por batch (default: 5000)
  --parallel <n>          Tablas en paralelo (default: 4)
  --encrypt               Forzar cifrado (default: off)
  --dry-run               Validar sin mover datos
  --on-conflict <strategy>  skip | overwrite | error (default: error)

# tos sync
tos sync
  --from <uri>
  --to   <uri>            (repetible)
  --table <name>          (repetible)
  --watch                 Activar modo real-time
  --initial-sync          Hacer push completo antes del watch (default: true)
  --encrypt
  --reconnect-delay <ms>  Delay entre reconexiones (default: 1000)

# tos schema
tos schema pull <uri>            Exportar schema como SDL
tos schema push <file> --to <uri>  Aplicar SDL en destino
tos schema infer --from <uri>    Inferir SDL desde fuente
tos schema diff <file1> <file2>  Comparar dos SDLs
tos schema validate <file>       Validar SDL

# tos node
tos node start      Iniciar daemon P2P (escucha conexiones entrantes)
tos node stop
tos node status
tos node id         Mostrar node_id y public_key

# tos status
tos status          Sesiones activas, bytes transferidos, lag en sync

# tos log
tos log             Historial de transferencias
tos log --follow    Tail en tiempo real
```

### Ejemplos Reales

```bash
# Migrar MySQL legacy a PostgreSQL
tos push --from mysql://admin:pass@192.168.1.10/legacy_db \
         --to   postgres://admin:pass@192.168.1.20/new_db

# Cache invalidation: PostgreSQL → Redis en tiempo real
tos sync --from postgres://db:5432/production \
         --to   redis://cache:6379 \
         --table users --table products \
         --watch

# Backup automático a JSON
tos sync --from postgres://db/production \
         --to   json:///mnt/backups/production.json \
         --watch

# Multi-nodo: PostgreSQL → Redis + JSON + ClickHouse
tos sync --from postgres://db/prod \
         --to   redis://cache \
         --to   json:///backup/prod.json \
         --to   clickhouse://analytics/prod \
         --watch --encrypt

# Solo schema, sin datos
tos schema pull postgres://db/prod | tos schema push --to mysql://db/staging

# Inferir schema de un JSON y migrar a PostgreSQL
tos schema infer --from json:///users.json > users.tos
tos push --from json:///users.json \
         --to   postgres://db/prod \
         --schema users.tos
```

---

## Estructura del Proyecto

```
tos/
├── Cargo.toml               # workspace
├── README.md
├── LICENSE                  # Apache 2.0
├── PROJECT.md               # este archivo
│
├── tos-core/                # Capa 1: SDL + Type System
│   ├── src/
│   │   ├── lib.rs
│   │   ├── sdl/             # Parser SDL (TOML → TosSchema)
│   │   │   ├── parser.rs
│   │   │   ├── schema.rs    # structs: TosSchema, TosField, TosType
│   │   │   └── infer.rs     # inferencia de schema desde datos
│   │   ├── types/           # Sistema de tipos universal
│   │   │   ├── primitive.rs
│   │   │   ├── compound.rs
│   │   │   └── resolve.rs   # resolución de conflictos
│   │   └── error.rs
│
├── tos-wire/                # Capa 2: Wire Format
│   ├── src/
│   │   ├── lib.rs
│   │   ├── batch.rs         # serialización/deserialización de batches
│   │   ├── change.rs        # formato de CHANGE records
│   │   ├── arrow.rs         # Arrow IPC integration
│   │   └── msgpack.rs       # MessagePack fallback
│
├── tos-crypto/              # Criptografía (usada por proto y adapters)
│   ├── src/
│   │   ├── lib.rs
│   │   ├── identity.rs      # keypair Ed25519, node_id
│   │   ├── sign.rs          # sign/verify
│   │   ├── exchange.rs      # X25519 ECDH
│   │   ├── encrypt.rs       # ChaCha20-Poly1305
│   │   └── hash.rs          # BLAKE3 wrappers
│
├── tos-proto/               # Capa 3: Protocolo P2P
│   ├── src/
│   │   ├── lib.rs
│   │   ├── messages.rs      # definición de todos los mensajes
│   │   ├── handshake.rs     # HELLO / HELLO_ACK
│   │   ├── session.rs       # gestión de sesión completa
│   │   ├── stream.rs        # STREAM_START / BATCH / ACK / STREAM_END
│   │   ├── watch.rs         # CHANGE / CHANGE_ACK
│   │   ├── topology.rs      # multi-nodo: fan-out, merge, chain
│   │   └── transport.rs     # QUIC via quinn
│
├── tos-adapters/            # Capa 4: Adaptadores por DB
│   ├── postgres/
│   │   ├── src/
│   │   │   ├── adapter.rs   # impl TosAdapter
│   │   │   ├── types.rs     # mapeo tipos PG ↔ ToS
│   │   │   ├── schema.rs    # leer/escribir schema PG
│   │   │   ├── stream.rs    # leer/escribir datos
│   │   │   └── watch.rs     # logical replication / LISTEN
│   ├── mysql/
│   ├── mongodb/
│   ├── redis/
│   ├── sqlite/
│   ├── json/
│   ├── yaml/
│   └── txt/
│
├── tos-cli/                 # Capa 5: CLI
│   ├── src/
│   │   ├── main.rs
│   │   ├── cmd/
│   │   │   ├── push.rs
│   │   │   ├── sync.rs
│   │   │   ├── schema.rs
│   │   │   ├── topology.rs
│   │   │   ├── node.rs
│   │   │   ├── status.rs
│   │   │   └── log.rs
│   │   └── config.rs        # ~/.tos/config.toml
│
└── tests/
    ├── integration/         # tests E2E entre adaptadores reales
    │   ├── pg_to_redis.rs
    │   ├── mysql_to_postgres.rs
    │   ├── json_to_sqlite.rs
    │   └── multinode.rs
    └── fixtures/            # schemas y datos de prueba
```

---

## Decisiones de Diseño

### Rust sobre Go, C, Python

- **Performance**: zero-cost abstractions, sin GC pauses — crítico para streaming de alta frecuencia
- **Seguridad de memoria**: el protocolo maneja datos de producción de clientes
- **Ecosistema async**: tokio + quinn para async P2P es la combinación más madura en Rust
- **ARM**: Rust tiene excelente soporte para ARM64/ARMv7, incluyendo Termux
- **Go**: hubiera sido opción válida, pero el type system de Rust es mejor para el sistema de tipos universal
- **C**: demasiado manual para un proyecto de este scope, especialmente crypto

### Por qué no hay servidor central / registry

ToS es un protocolo, no un servicio. Un servidor central contradice el principio P2P. La identidad es el node_id derivado del keypair. Los nodos se descubren por URI directo. No hay DNS central de ToS. Si la comunidad quiere un registry, lo construye encima — no es parte del protocolo core.

### Por qué hash + firma opcional y no mTLS

mTLS requiere:
1. PKI propia o CA pública
2. Certificados con expiración
3. Renovación periódica
4. Configuración no trivial para usuarios no-expertos

El modelo Ed25519 keypair es:
1. Generado automáticamente en primer arranque
2. Sin expiración (rotación es decisión del operador)
3. Sin CA, sin registry
4. Funciona en ARM, en Termux, en un VPS de $5

La seguridad es equivalente para el threat model de ToS (integridad de datos en tránsito). mTLS sería overkill.

### Por qué BLAKE3 sobre SHA-256

- BLAKE3 es 3-5x más rápido que SHA-256 en ARM sin instrucciones hardware SHA
- En el Redmi 12 (Snapdragon 685), esta diferencia es material
- BLAKE3 es resistente a length extension attacks (SHA-256 no sin HMAC)
- Misma seguridad (256 bits) con mejor performance

### Por qué ChaCha20-Poly1305 sobre AES-GCM

- AES-GCM requiere instrucciones hardware AES para ser rápido (AES-NI)
- ARM Cortex-A53 (Snapdragon 685) no tiene AES-NI
- ChaCha20 es puro software, igualmente rápido en cualquier CPU
- ChaCha20-Poly1305 es el algoritmo de TLS 1.3 por defecto en mobile

### Por qué el SDL es TOML y no YAML

YAML tiene quirks bien documentados: `yes`, `no`, `on`, `off` se parsean como booleanos en versiones pre-1.2. Los números octales ambiguos (`0777`). El Norway Problem (`NO` = `false`). TOML es más estricto y predecible.

### Por qué TXT/JSON/YAML son "bases de datos" en ToS

El criterio de ToS no es "¿es una base de datos real?". El criterio es "¿tiene schema o puede inferirse uno?". Un archivo JSON de 100k registros es funcionalmente una base de datos. Excluirlo crearía una barrera innecesaria y excluiría a una fracción importante de usuarios reales con datos legítimos.

### Por qué la metáfora de la tos

La tos se propaga de persona a persona sin servidor central. Cada nodo que "recibe" los datos puede a su vez propagarlos. En una topología chain, los datos se "contagian" a través de la red. La metáfora describe el comportamiento multi-nodo de forma intuitiva y memorable.

---

## Roadmap

> Plan detallado para llevar ToS desde cero hasta la versión `1.0.0` estable. Cada tarea tiene un **ID estable** (T-NNN) que se referencia en commits, issues y PRs.

---

### 0. Convenciones del plan

- **T-shirt sizing**: `S` ≤ ½ día · `M` = 1 día · `L` = 2–3 días · `XL` ≥ 1 semana
- **"Done" por tarea**: código mergeado + tests pasando + `cargo clippy -- -D warnings` limpio + rustdoc en APIs públicas
- **Convención de commits**: `feat(tos-core): T-012 parser SDL TOML`
- **Versionado SemVer**: `0.1.0`, `0.5.0`, `1.0.0` corresponden a hitos con release binario
- **Trunk-based**: `main` siempre verde; features en branches cortos con PR

---

### 1. Pre-requisitos (una sola vez, antes de T-001)

| ID    | Tarea                                                                                  | S/M/L | Entregable                                                                  |
| ----- | -------------------------------------------------------------------------------------- | ----- | --------------------------------------------------------------------------- |
| T-000 | Crear workspace `~/tos/` con `Cargo.toml` workspace y miembros vacíos                  | S     | `cargo build --workspace` ok con un `lib.rs` por crate                       |
| T-001 | `git init`, `LICENSE` (Apache-2.0), `README.md`, `.gitignore` Rust                      | S     | Repo con primer commit                                                      |
| T-002 | `.cargo/config.toml` con targets ARM64, ARMv7, x86_64 y toolchains instaladas          | M     | `cross build --target aarch64-unknown-linux-musl` compila                   |
| T-003 | GitHub Actions: matrix build (linux/amd64, linux/arm64, macos, windows) + clippy       | M     | PR abre CI verde en 4 OS                                                    |
| T-004 | Instalar `cross`, `cargo-nextest`, `cargo-deny`, `sqlx-cli`, `mongod` para tests       | S     | Binarios en `$PATH`                                                         |
| T-005 | Crear `tests/fixtures/` con datasets pequeños (users.csv, products.json, schema.tos)    | S     | Archivos versionados                                                        |
| T-006 | Definir `~/.tos/` layout (config.toml, identity, logs) en código                       | S     | Constantes + creación automática al primer `tos node start`                 |
| T-007 | Política de MSRV: Rust 1.75+ (estable desde dic-2023, compatible Termux)               | S     | `rust-toolchain.toml` con `channel = "1.75"`                                |

---

### 2. v0.1 — Proof of Concept — `0.1.0`

**Objetivo de la fase**: `tos push --from postgres://local/db --to json:///backup.json` funciona end-to-end sobre TCP loopback con un único `users` table.

#### 2.1 tos-core (capa 1)

| ID    | Tarea                                                                                | S/M/L | Deps    | Entregable / Criterio de aceptación                                                                  |
| ----- | ------------------------------------------------------------------------------------ | ----- | ------- | ----------------------------------------------------------------------------------------------------- |
| T-010 | Definir structs `TosType`, `TosField`, `TosSchema` en `types/primitive.rs`           | M     | T-000   | Enums + serde; tests unitarios de Display/Debug/PartialEq                                            |
| T-011 | `types/compound.rs`: `optional<T>`, `array<T>`, `map<K,V>`, `enum`, `union`          | M     | T-010   | Cada variante construible y deconstruible; tests                                                     |
| T-012 | Parser SDL TOML en `sdl/parser.rs` (usa `toml` crate)                                | L     | T-010   | `parse_sdl("schema.tos") -> TosSchema`; roundtrip parse→serialize→parse == id; errores con línea+col |
| T-013 | Validador de SDL: nombres válidos, no duplicate fields, refs forward-checked         | M     | T-012   | `validate(&TosSchema) -> Result<(), Vec<ValidationError>>` con codes de error                       |
| T-014 | `sdl/infer.rs`: inferencia básica desde JSON/CSV (solo tipos primitivos)            | L     | T-010   | `infer_schema(json_value) -> TosSchema`; tests con casos del doc (sección SDL)                       |
| T-015 | Resolución de tipos: tabla de mapeos lossless entre nativos (PG↔JSON por ahora)      | M     | T-011   | `resolve_native(pg_type, json_type) -> Resolution` con flag Lossy/OK                                 |

#### 2.2 tos-wire (capa 2)

| ID    | Tarea                                                          | S/M/L | Deps    | Entregable                                                                  |
| ----- | -------------------------------------------------------------- | ----- | ------- | --------------------------------------------------------------------------- |
| T-020 | Estructura `BatchHeader` de 20 bytes con `bincode`             | M     | T-000   | Serializar/deserializar roundtrip; test con bytes conocidos                 |
| T-021 | Serialización MessagePack de records (usa `rmp-serde`)        | M     | T-020   | `encode_batch(records) -> Vec<u8>`; bench > 100k rec/s en Redmi 12           |
| T-022 | Hash BLAKE3 del batch plaintext en header                      | S     | T-020   | Campo `batch_hash` se computa y verifica en roundtrip                       |
| T-023 | `encode_change` / `decode_change` para `ChangeRecord`         | M     | T-020   | Tests con INSERT/UPDATE/DELETE binarios                                     |

#### 2.3 tos-crypto

| ID    | Tarea                                                          | S/M/L | Deps    | Entregable                                                                  |
| ----- | -------------------------------------------------------------- | ----- | ------- | --------------------------------------------------------------------------- |
| T-030 | `identity.rs`: keypair Ed25519 + `node_id = BLAKE3(pk)`        | S     | T-001   | `Identity::generate() -> Self`; persistencia en `~/.tos/identity` (0600)    |
| T-031 | `sign.rs`: `sign(hash)` y `verify(pk, hash, sig)`              | S     | T-030   | Tests con vectores de prueba de ed25519-dalek                               |
| T-032 | `hash.rs`: wrapper BLAKE3 con API uniforme                    | S     | T-001   | `hash_bytes(&[u8]) -> [u8;32]`                                              |

#### 2.4 tos-proto (capa 3, **TCP en v0.1**, QUIC en v0.5)

| ID    | Tarea                                                                                                          | S/M/L | Deps              | Entregable                                                              |
| ----- | -------------------------------------------------------------------------------------------------------------- | ----- | ----------------- | ----------------------------------------------------------------------- |
| T-040 | Mensajes en `messages.rs` con `bincode` (HELLO, HELLO_ACK, SCHEMA_OFFER, BATCH, ACK, STREAM_END, DONE)         | L     | T-031, T-020      | Structs serde; tests de encode/decode roundtrip                        |
| T-041 | `handshake.rs`: async handshake con `tokio::net::TcpStream`                                                    | L     | T-040             | `handshake(a, b) -> Session` con test loopback 127.0.0.1                |
| T-042 | `stream.rs`: STREAM_START → loop BATCH/ACK → STREAM_END → DONE                                                 | L     | T-040             | Test: 1000 records en batches de 100 → 10 ACKs                          |
| T-043 | `transport.rs`: trait `Transport` con impl `TcpTransport` (swap a QUIC en v0.5)                                | M     | T-040             | `TcpTransport::connect/accept` retorna `Stream + Sink` de bytes         |
| T-044 | `session.rs`: orquesta el ciclo completo, mide `total_records` / `duration_ms`                                 | M     | T-041, T-042      | `Session::run() -> SessionStats` con métricas para `tos status`         |

#### 2.5 adapters

| ID    | Tarea                                                                                  | S/M/L | Deps    | Entregable                                                                  |
| ----- | -------------------------------------------------------------------------------------- | ----- | ------- | --------------------------------------------------------------------------- |
| T-050 | `tos-adapters/postgres/`: impl `TosAdapter` para `read_schema` (lee `information_schema`) | M | T-010   | `pg::read_schema(conn) -> TosSchema` contra PG 15+ en Docker                 |
| T-051 | `pg`: `read_records` con cursor + protocolo COPY para velocidad                        | L     | T-050   | Stream de records tipados; bench vs. SELECT * por 1M rows                    |
| T-052 | `tos-adapters/json/`: read + write schema y records                                    | M     | T-010   | `JsonAdapter` con inferencia y pretty-print opcional                        |
| T-053 | `json`: CLI: `--from json://` resuelve path a `JsonAdapter`                            | S     | T-052   | URI parser con schemes `json://`, `postgres://`                             |

#### 2.6 tos-cli

| ID    | Tarea                                                                          | S/M/L | Deps                       | Entregable                                                                                  |
| ----- | ------------------------------------------------------------------------------ | ----- | -------------------------- | ------------------------------------------------------------------------------------------- |
| T-060 | `tos-cli/src/main.rs` con clap v4, subcomandos                                  | M     | T-000                      | `tos --help` muestra `push`, `sync`, `schema`, `topology`, `node`, `status`, `log`            |
| T-061 | `cmd/push.rs`: parse args, conecta fuente, llama `Session::run`                  | M     | T-060, T-044, T-051, T-052 | `tos push --from postgres://u:p@/db --to json:///tmp/out.json` produce JSON con datos      |
| T-062 | Reporter de progreso: barra con `indicatif`, tasa, ETA                          | S     | T-061                      | Output legible en terminal y `--json` flag para máquinas                                    |

#### 2.7 Criterios "Done" de v0.1 (release `0.1.0`)

- [ ] Test E2E `tests/integration/pg_to_json.rs` pasa: PG con tabla `users` (10k filas) → JSON con todos los records
- [ ] Integridad: alterar 1 byte en tránsito → NACK + retransmisión detectada
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` limpio
- [ ] `cargo test --workspace` 100% verde
- [ ] Binario `tos` corre en Termux (ARM64) sin flags especiales
- [ ] `README.md` con quickstart de 5 líneas

---

### 3. v0.5 — Beta — `0.5.0`

**Objetivo**: cache invalidation real en producción — `tos sync --from postgres://prod --to redis://cache --watch --table users` propaga cambios sub-segundo.

#### 3.1 Migración a QUIC

| ID    | Tarea                                                          | S/M/L | Deps    | Entregable                                                                  |
| ----- | -------------------------------------------------------------- | ----- | ------- | --------------------------------------------------------------------------- |
| T-100 | `transport.rs`: nueva impl `QuicTransport` con `quinn`         | XL    | T-043   | `QuicTransport::connect/accept` con self-signed certs al boot               |
| T-101 | Multiplexing: 1 conexión QUIC, N streams (1 por sesión de tabla) | M    | T-100   | Cada tabla sync usa un stream independiente sin HOL blocking                |
| T-102 | Cert pinning opcional: `tos-cli` puede persistir remote pk     | M     | T-100   | Trust on first use, warning si cambia                                       |
| T-103 | Mantener `TcpTransport` detrás de feature flag `tcp-fallback`  | S     | T-100   | Default sin flag usa QUIC                                                   |

#### 3.2 adapters adicionales

| ID    | Tarea                                                                                | S/M/L | Deps    | Entregable                                                                  |
| ----- | ------------------------------------------------------------------------------------ | ----- | ------- | --------------------------------------------------------------------------- |
| T-110 | `pg`: `write_schema` + `write_records` (CREATE TABLE + COPY)                         | M     | T-050   | Aplica SDL a PG destino; test con schemas con constraints                    |
| T-111 | `pg`: `watch` con `pgoutput` / logical replication slot                              | XL    | T-110   | `CREATE PUBLICATION tos_pub`, `pgoutput` decode → `ChangeStream`            |
| T-112 | `pg`: fallback watch con `LISTEN/NOTIFY` (logical no disponible)                     | L     | T-110   | Trigger opcional que NOTIFY cambios como JSON                                |
| T-113 | `tos-adapters/redis/`: read/write de hashes y streams                                | M     | T-010   | `RedisAdapter` con `HGETALL` estructura y `HSET` pipeline                   |
| T-114 | `redis`: `watch` con keyspace notifications                                          | M     | T-113   | `CONFIG SET notify-keyspace-events KEA` + PSUBSCRIBE → `ChangeStream`        |
| T-115 | `tos-adapters/mysql/`: read_schema (information_schema) + read_records (cursor)      | M     | T-010   | `MysqlAdapter` testado contra MySQL 8 y MariaDB 10.11                       |
| T-116 | `mysql`: `write_schema` + `write_records` (LOAD DATA INFILE)                         | M     | T-115   | Usa el protocolo binario nativo de MySQL para velocidad                     |
| T-117 | `json`: `watch` con `notify` crate (inotify en Linux, FSEvents en macOS)             | L     | T-052   | Detecta cambios en `.json` y emite `ChangeOp::Update`                       |

#### 3.3 tos-core v0.5

| ID    | Tarea                                                                                                  | S/M/L | Deps         | Entregable                                                                  |
| ----- | ------------------------------------------------------------------------------------------------------ | ----- | ------------ | --------------------------------------------------------------------------- |
| T-120 | Serialización wire para tipos compuestos (`array<T>`, `map<K,V>`)                                       | M     | T-011, T-020 | Tests con arrays anidados, map de map                                       |
| T-121 | `resolve.rs`: 4 niveles (Lossless, Lossy+warning, Reject, Custom resolver)                             | L     | T-015        | `TypeResolver::resolve(from, to, policy) -> Resolution` con `policy.toml`   |
| T-122 | `sdl/infer.rs`: también desde YAML y TXT con delimitador                                               | M     | T-014        | `infer_schema_csv(path, delim, has_header) -> TosSchema`                    |

#### 3.4 tos-proto v0.5

| ID    | Tarea                                                                                  | S/M/L | Deps              | Entregable                                                                  |
| ----- | -------------------------------------------------------------------------------------- | ----- | ----------------- | --------------------------------------------------------------------------- |
| T-130 | Mensajes `SCHEMA_DIFF`, `SCHEMA_CONFIRM`, `CHANGE`, `CHANGE_ACK` con serialización      | M     | T-040             | Roundtrip tests                                                             |
| T-131 | `watch.rs`: loop continuo emitiendo `CHANGE` por cada `ChangeOp` recibido del adapter   | L     | T-130, T-111      | Tests con mock `ChangeStream` que emite 1000 cambios → 1000 ACKs            |
| T-132 | Negociación de schema: enviar SDL, recibir diff, confirmar o abortar                    | L     | T-040, T-121      | Test: PG `int8` vs MySQL `BIGINT UNSIGNED` → `int64` con warning            |
| T-133 | Reconnect automático con backoff exponencial (default 1s, máx 30s)                      | M     | T-100             | Session se reconecta sin perder estado; WATCH events buffered en disco      |

#### 3.5 tos-cli v0.5

| ID    | Tarea                                                          | S/M/L | Deps    | Entregable                                                                  |
| ----- | -------------------------------------------------------------- | ----- | ------- | --------------------------------------------------------------------------- |
| T-140 | `cmd/sync.rs`: `tos sync` con `--watch`, `--initial-sync`      | M     | T-061   | Puede hacer push inicial y luego watch                                      |
| T-141 | Flag `--to` repetible (fan-out a múltiples destinos)           | S     | T-140   | Test: 1 PG → 3 destinos simultáneos                                         |
| T-142 | Reporter: muestra lag de watch en ms, throughput sostenido     | S     | T-140   | `tos status` muestra por sesión                                            |

#### 3.6 Criterios "Done" de v0.5 (release `0.5.0`)

- [ ] Test E2E `pg_to_redis.rs` real: insertar fila en PG → HSET aparece en Redis en <500 ms p99
- [ ] Test E2E `mysql_to_postgres.rs`: migración 1M rows, 0 pérdida (verificada con CHECKSUM)
- [ ] `--encrypt` opcional compila y handshake negocia cifrado
- [ ] `tos sync --watch` sobrevive kill+restart del nodo destino sin perder eventos (T-133)
- [ ] CI pasa en matrix linux/amd64 + linux/arm64 + macOS + windows
- [ ] `docs/book/` con tutorial "5 minutos para tu primer sync"

---

### 4. v1.0 — Estable — `1.0.0`

**Objetivo**: protocolo estable, adapters maduros, topologías multi-nodo operacionales, binarios para las 4 plataformas target.

#### 4.1 adapters restantes

| ID    | Tarea                                                                                | S/M/L | Deps    | Entregable                                                                  |
| ----- | ------------------------------------------------------------------------------------ | ----- | ------- | --------------------------------------------------------------------------- |
| T-200 | `tos-adapters/sqlite/`: read/write schema y records, sin watch                      | M     | T-010   | `SqliteAdapter` con `rusqlite`; test en memory + on-disk                     |
| T-201 | `tos-adapters/mongodb/`: read_schema (listCollections + sample)                     | L     | T-010   | Decodifica BSON → TosType; mapea ObjectId → uuid                            |
| T-202 | `mongodb`: write_schema + write_records (insertMany)                                | M     | T-201   |                                                                              |
| T-203 | `mongodb`: `watch` con change streams (oplog tailing)                              | XL    | T-202   | Requiere replica set; documentar prerequisito                                |
| T-204 | `tos-adapters/yaml/`: read/write + watch (inotify)                                  | M     | T-052   | Reusa `notify` de T-117                                                     |
| T-205 | `tos-adapters/txt/`: read/write CSV con config de delimitador                       | M     | T-010   | Soporta quote_char, null_str, encoding                                      |
| T-206 | `txt`: `watch` (inotify en directorio)                                              | S     | T-205   |                                                                              |
| T-207 | `tos-adapters/csv/`: variante de txt optimizada para export                         | S     | T-205   | Streaming write sin buffering completo en memoria                            |

#### 4.2 Topologías multi-nodo

| ID    | Tarea                                                                              | S/M/L | Deps            | Entregable                                                                  |
| ----- | ---------------------------------------------------------------------------------- | ----- | --------------- | --------------------------------------------------------------------------- |
| T-210 | `topology.rs` en tos-proto: parser de `tos-topology.toml` (vec<node>, vec<edge>)   | M     | T-140           | `Topology::from_file(path) -> Topology` validado                            |
| T-211 | Fan-out: 1 source → N destinos con sesiones paralelas                              | L     | T-141, T-210    | `tos topology --file` ejecuta el grafo                                      |
| T-212 | Merge: N sources → 1 destino, deduplicación por primary key                        | XL    | T-211           | Política: last-write-wins / source-priority / error                          |
| T-213 | Chain: A→B→C, cada nodo actúa como source para el siguiente                        | L     | T-211           | DONE en A dispara STREAM_START en B                                         |
| T-214 | Mesh: heartbeat 5s entre nodos, dead-node detection                                 | M     | T-211           | Si un nodo no responde en 30s, marca degradado y reintenta                  |

#### 4.3 Cifrado y crypto final

| ID    | Tarea                                                                              | S/M/L | Deps              | Entregable                                                                  |
| ----- | ---------------------------------------------------------------------------------- | ----- | ----------------- | --------------------------------------------------------------------------- |
| T-220 | `crypto/exchange.rs`: X25519 ECDH ephemeral keypair                                | M     | T-031             | `derive_session_key(my_priv, their_pub) -> [u8;32]`                          |
| T-221 | `crypto/encrypt.rs`: ChaCha20-Poly1305 AEAD                                        | M     | T-220             | `encrypt(plain, key, nonce) -> ciphertext_with_tag`; tests RFC 7539          |
| T-222 | Integrar en handshake: `if encrypt { derive + encrypt stream }`                    | M     | T-100, T-221      | `tos sync --encrypt` E2E + sniff test (0 bytes plaintext en wire)           |
| T-223 | Key rotation: nuevo ephemeral cada 1h o 1 GB transferido                            | L     | T-222             | Sin interrumpir streams activos                                             |

#### 4.4 Wire format v1.0

| ID    | Tarea                                                                              | S/M/L | Deps             | Entregable                                                                  |
| ----- | ---------------------------------------------------------------------------------- | ----- | ---------------- | --------------------------------------------------------------------------- |
| T-230 | `wire/arrow.rs`: integración con `arrow` + `arrow_ipc`                             | XL    | T-020            | `encode_arrow(records) -> Vec<u8>` con schema Arrow embebido                |
| T-231 | Selector automático en `BatchHeader`: si count > 10k y types compatibles → Arrow   | M     | T-230, T-021     | Heurística de la sección Wire Format implementada                            |
| T-232 | Backwards compat: receiver anuncia en HELLO_ACK los formatos que soporta          | S     | T-040            | Negociación simple                                                          |

#### 4.5 Schema tooling completo

| ID    | Tarea                                                                              | S/M/L | Deps                                | Entregable                                                                  |
| ----- | ---------------------------------------------------------------------------------- | ----- | ----------------------------------- | --------------------------------------------------------------------------- |
| T-240 | `cmd/schema.rs`: `pull` (lee de DB, emite SDL)                                     | M     | T-110, T-115, T-200, T-201          | `tos schema pull postgres://...` funciona para todos los adapters            |
| T-241 | `cmd/schema.rs`: `push` (lee SDL, aplica a DB)                                     | M     | T-110                               | `tos schema push schema.tos --to mysql://...`                              |
| T-242 | `cmd/schema.rs`: `infer` (desde JSON/YAML/TXT)                                     | S     | T-122                               |                                                                              |
| T-243 | `cmd/schema.rs`: `diff` (compara 2 SDL, lista diferencias)                         | M     | T-013                               | Output tipo unified diff                                                    |
| T-244 | `cmd/schema.rs`: `validate` (solo valida sin conectar)                             | S     | T-013                               | `tos schema validate foo.tos` exit 0/1                                      |

#### 4.6 Daemon y ops

| ID    | Tarea                                                                              | S/M/L | Deps    | Entregable                                                                  |
| ----- | ---------------------------------------------------------------------------------- | ----- | ------- | --------------------------------------------------------------------------- |
| T-250 | `cmd/node.rs`: `tos node start` daemoniza, escucha conexiones QUIC entrantes       | L     | T-100   | PID file, log a syslog / journald                                            |
| T-251 | `cmd/node.rs`: `stop`, `status`, `id` (muestra node_id y pubkey)                  | S     | T-250   |                                                                              |
| T-252 | `~/.tos/config.toml` con defaults (límite memoria, batch size, log level)         | S     | T-250   | `tos config` para editarlo interactivamente                                  |
| T-253 | `cmd/status.rs`: sesiones activas con bytes transferidos, lag, uptime              | M     | T-044   | TUI ligera con `ratatui` opcional (default: tabla plain text)               |
| T-254 | `cmd/log.rs`: historial de transfers con `--follow` (tail)                         | M     | T-250   | Logs estructurados JSON en `~/.tos/logs/`, parseables por `jq`               |

#### 4.7 Release y packaging

| ID    | Tarea                                                                              | S/M/L | Deps    | Entregable                                                                  |
| ----- | ---------------------------------------------------------------------------------- | ----- | ------- | --------------------------------------------------------------------------- |
| T-260 | Workflow release en GitHub Actions: tag `v*` → cross-compile a 4 targets          | L     | T-002   | Binarios adjuntos al GitHub Release con SHA256SUMS                          |
| T-261 | Target list: `aarch64-unknown-linux-musl`, `armv7-unknown-linux-musleabihf`, `x86_64-unknown-linux-musl`, `x86_64-pc-windows-msvc` | M | T-260 | Script `scripts/release.sh`                                                |
| T-262 | Paquetes: `cargo deb`, `cargo rpm` para linux; MSI para windows                   | L     | T-260   | `tos` instalable vía `apt` / `dnf` / `winget`                                |
| T-263 | Homebrew tap `opceanai/tap` con formula `tos`                                      | S     | T-260   | `brew install opceanai/tap/tos`                                              |
| T-264 | Docker image `ghcr.io/opceanai/tos:1.0.0` con entrypoint daemon                    | M     | T-250   | `docker run -v ~/.tos:/root/.tos opceanai/tos node start`                    |

#### 4.8 Criterios "Done" de v1.0 (release `1.0.0`)

- [ ] Spec congelada: este `PROJECT.md` v1.0, sin breaking changes prometidos durante 12 meses
- [ ] 8 adapters funcionales: PG, MySQL, Mongo, Redis, SQLite, JSON, YAML, TXT/CSV
- [ ] Topologías: fan-out, merge, chain, mesh todas con tests E2E
- [ ] Cifrado: `--encrypt` end-to-end con test que sniffea el wire y confirma 0 bytes de plaintext
- [ ] Arrow IPC para batches >10k records con bench público
- [ ] Daemon mode estable con systemd unit y ejemplo de docker-compose
- [ ] Binarios publicados para ARM64, ARMv7, x86_64-linux, x86_64-windows con SHA256SUMS firmados
- [ ] Cobertura de tests ≥70% en `tos-core`, `tos-wire`, `tos-crypto`, `tos-proto`
- [ ] `docs/book/` (mdBook) con 5 tutoriales completos
- [ ] Fuzz testing básico: `cargo fuzz` en parser SDL y decoder de wire
- [ ] Security audit: `cargo audit` sin warnings; revisión manual del handshake
- [ ] `CHANGELOG.md` con todas las features desde `0.5.0`

---

### 5. Cross-cutting (continuo, durante todas las fases)

Estas tareas se hacen en cada PR / release, no son un hito aislado:

| ID    | Tarea                                                                 | Cuándo                              | Entregable                                                                  |
| ----- | --------------------------------------------------------------------- | ----------------------------------- | --------------------------------------------------------------------------- |
| T-900 | Tests unitarios por crate (objetivo ≥70% coverage en crates core)     | Cada PR                             | Reporte en CI con `cargo-llvm-cov`                                          |
| T-901 | Tests de integración con `testcontainers` (PG, MySQL, Redis, Mongo)   | En cada fase                        | Suite `tests/integration/` corre en CI con containers efímeros                |
| T-902 | `cargo clippy --workspace --all-targets -- -D warnings`               | Cada PR (gate)                      | CI falla si hay warning                                                     |
| T-903 | `cargo fmt --check` + `cargo deny check`                              | Cada PR                             | CI gate                                                                      |
| T-904 | Benchmarks con `criterion` en `tos-wire`, `tos-crypto`, `tos-proto`   | Cuando se toca performance crítico  | Reporte comparativo en CI (criterion-llvm)                                  |
| T-905 | Changelog automático con `git-cliff`                                  | En cada release                     | `CHANGELOG.md` regenerado                                                   |
| T-906 | Versionado sincronizado: `cargo workspaces` versiona todos los crates  | En cada release                     | Tags `tos-core@0.5.0`, etc.                                                  |
| T-907 | Documentación rustdoc publicada en GitHub Pages con `docs.rs`         | Continuo                            | `cargo doc --no-deps --all-features` sin warnings                            |
| T-908 | `cargo msrv verify` en CI para garantizar Rust 1.75+                  | En cada PR                          |                                                                              |
| T-909 | Fuzzing continuo con `cargo-fuzz` (SDL parser, wire decoder)          | Desde v0.5                          | Corpus persistido, 1 hora de fuzz en nightly CI                              |

---

### 6. Post v1.0 (backlog, no detallado)

Lista corta para mantener visibilidad; no se planifica en detalle hasta que `1.0.0` esté released:

- Adapters: Parquet, Arrow IPC directo, ClickHouse, Cassandra/ScyllaDB, REST API (cliente), S3/GCS
- SDKs: Go, Python, Node.js
- Plugin system para adaptadores de terceros (dynamic loading o WASM)
- GUI desktop con Tauri para visualizar topologías
- `tos discover`: mDNS / DNS-SD para encontrar nodos en LAN
- Compresión opcional en wire (zstd)
- Schema evolution automática: si el emisor cambia SDL, el receptor migra el destino sin downtime

---

### 7. Riesgos y mitigaciones

| #  | Riesgo                                                                                       | Probabilidad | Impacto | Mitigación                                                                                              |
| -- | -------------------------------------------------------------------------------------------- | ------------ | ------- | ------------------------------------------------------------------------------------------------------- |
| R1 | `quinn` o `arrow` crate rompe API entre versiones                                            | Media        | Alto    | Pin minor version, actualizar con PR dedicado, tests de integración en CI                              |
| R2 | Logical replication de PG requiere permisos especiales y `wal_level=logical`                  | Alta         | Medio   | T-112 fallback con LISTEN/NOTIFY; documentar prerequisito; warning si logical no disponible             |
| R3 | Change streams de Mongo requieren replica set (no standalone)                                | Alta         | Medio   | Documentar; detectar en `tos schema pull` y emitir error claro                                          |
| R4 | Performance en Redmi 12 no alcanza throughput prometido                                       | Media        | Alto    | Benchmarks desde v0.1; fallback a TCP si QUIC consume demasiada CPU; optimizaciones con `cargo-bloat`   |
| R5 | Conflicto de schema en merge multi-source genera datos inconsistentes                         | Alta         | Alto    | T-212 con policy explícita; default a `error` (no silencioso); tests E2E con conflictos forzados        |
| R6 | QUIC NAT traversal no funciona en redes restrictivas                                          | Media        | Medio   | Soporte de relay opcional (post-v1.0); documentar workarounds con Tailscale/WireGuard                   |
| R7 | Cross-compile a ARMv7 con `musleabihf` requiere linker externo                                | Alta         | Bajo    | Script `scripts/install-arm-toolchain.sh` con instrucciones; CI usa `cross`                             |
| R8 | Ed25519 keypair robado = compromiso permanente                                                | Baja         | Alto    | Documentar rotación manual; keypair cifrado en disco con passphrase (post-v1.0)                         |
| R9 | Cifrado opcional crea split-brain: 2 nodos uno cifra, otro no                                  | Media        | Alto    | Negociación estricta: si A ofrece encrypt, B debe aceptarlo o rechazar el handshake                     |

---

### 8. Definición de "Done" global del proyecto

El proyecto v1.0 se considera completo cuando **todos** los puntos de §4.8 están en verde **y**:

- 3 implementaciones independientes del wire format han interoperado con éxito (validador cross-implementation)
- 1 usuario externo (no-OpceanAI) ha corrido `tos sync --watch` en producción por ≥30 días sin intervención
- Auditoría de seguridad externa (pagada o voluntaria) sin issues críticos
- `docs/book/` traducido a inglés + español

---

### Estimación agregada (T-shirt totals)

| Fase  | S   | M   | L   | XL  | Días-persona aprox (S=0.5, M=1, L=2.5, XL=5) |
| ----- | --- | --- | --- | --- | -------------------------------------------- |
| v0.1  | 8   | 12  | 6   | 0   | ~29 días                                     |
| v0.5  | 4   | 8   | 6   | 2   | ~30 días                                     |
| v1.0  | 6   | 13  | 8   | 3   | ~46 días                                     |
| Cross | —   | —   | —   | —   | ~5 días/mes ongoing                          |
| **Total** | **18** | **33** | **20** | **5** | **~105 días netos** (~5 meses a tiempo completo) |

> "Días-persona" asume 1 dev full-time senior Rust. Con interrupciones, onboarding y debugging inesperado, multiplicar ×2–2.5 para calendario real.

---

## Licencia

**Apache 2.0**

Por qué Apache 2.0 y no MIT:
- Apache 2.0 incluye cláusula de patentes explícita — protege a la comunidad de patent trolls
- Compatible con GPL (si alguien fork con GPL, pueden)
- Estándar en infraestructura (Kafka, Kubernetes, Rust stdlib)
- MIT no tiene protección de patentes

---

## OpceanAI

ToS es un proyecto de [OpceanAI](https://opceanai.com).

Creado por Agua. Arquitectura abierta. Zero budget. Construido desde un Redmi 12.

> *Barrier is mindset, not money.*
