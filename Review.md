# Code review: `GrpcError` + `with_error`

**Объём ревью:** `git diff 3d0a19c^..HEAD` — коммиты `3d0a19c`, `dd8ba19`, `1fed133`.

Изменения:

| Файл | Что |
|---|---|
| `my-grpc-extensions/src/grpc_error.rs` | новый публичный тип `GrpcError` (обёртка над `tonic::Status`) |
| `my-grpc-extensions/src/lib.rs` | `mod grpc_error; pub use grpc_error::*;` |
| `my-grpc-server-macros/src/generate_server/generate.rs` | новый параметр макроса `with_error: bool` |
| `README.md` | документация параметров `generate_server!`, `with_error` и его связки с `with_telemetry` |
| `my-grpc-extensions/Cargo.toml` | `0.7.0` → `0.7.1` в `3d0a19c`, затем откат обратно на `0.7.0` в `dd8ba19` |

**Метод:** чтение исходников, проверка зависимостей в `~/.cargo` (tonic 0.14.6, my-telemetry 1.3.0 rev `d184f4d`, types-reader 0.5.1), `cargo check --workspace --all-features`, ручное разворачивание генерируемого макросом кода с реальной сборкой и запуском.

---

## Что сделано правильно

Проверено сборкой, а не чтением.

* **`Ok(result?.into())` типизируется и работает.** tonic 0.14.6 предоставляет `impl<T> From<T> for Response<T>` (`tonic-0.14.6/src/response.rs:130`), поэтому собираются и `Response<T>`, и `Response<()>`. Прогон подтверждает семантику: `GrpcReadError::Timeout` → `DeadlineExceeded`, `GrpcReadError::TonicStatus(not_found("user 42"))` проходит насквозь с исходными кодом и сообщением.
* **Обратная совместимость реальная.** Без флага `unary_result_conversion` даёт ровно прежний `Ok(result.into())`, стримовая ветка (`result.get_result()`) не трогается вообще. Утверждение README на этот счёт верно.
* **Комментарий в README точен.** `use my_grpc_extensions::GrpcError; // the server prelude does not bring the bare name into scope` — `external-dependencies/src/lib.rs` реэкспортит только `Stream`, так что имя действительно не попадает в скоуп через `use #crate_ns::*;`.
* `cargo check --workspace --all-features` проходит; два предупреждения (`over_ssh`, `extract_domain_name`) существовали до этих коммитов.

Отдельно проверены и **отклонены как несостоятельные** претензии к: `From<String>`/`From<&str>` → `internal`, покрытию конструкторов `Status`, отсутствию `Error::source()`, «утечке» upstream-метаданных при пробросе `TonicStatus`, хардкоду метода `ping`, сосуществованию `GrpcServerStreamResult` и `StreamedResponseWriter`, а также тезису «`GrpcError` не оправдывает своего существования». Эти решения защитимы, менять их не нужно.

---

## Находки

### 1. HIGH — заголовок `process-id` роняет сервер паникой

`my-grpc-extensions/src/grpc_server_telemetry_context.rs:72-98`

`has_multiple_ids` срабатывает на любой запятой, а цикл разбора молча выбрасывает непарсящиеся сегменты. В результате строится `MyTelemetryContext::Multiple(vec![])`. В `Drop` он уходит в `write_success`, где у зафиксированной ревизии my-telemetry 1.3.0 (`d184f4d`, `my-telemetry-core/src/telemetry_interface.rs:56`) стоит:

```rust
for i in 0..ids.len() - 1 {
```

При пустом векторе это `0usize - 1`: паника «attempt to subtract with overflow» в debug; в release значение заворачивается в `usize::MAX` и паника происходит на `ids.get(..).unwrap()`. Паника внутри `Drop`.

**Триггер:** клиентский заголовок `process-id: ,` либо `process-id: abc,def`. Требуется `with_telemetry: true` и сконфигурированный telemetry writer.

**Область:** код существовал до этих коммитов (файл последний раз менялся в `be75a67`), но триггерится удалённо.

**Как чинить:** использовать парсер самой зависимости — `MyTelemetryContext::parse_from_string(process_id)` (`my-telemetry-core/src/ctx.rs:99`) возвращает `Result` и отвергает любой некорректный сегмент; на `Err` откатываться в `create_empty()`. Если оставлять свой цикл — добавить guard:

```rust
match ids.len() {
    0 => MyTelemetryContext::create_empty(),
    1 => MyTelemetryContext::Single(ids[0]),
    _ => MyTelemetryContext::Multiple(ids),
}
```

---

### 2. MEDIUM — упавший хендлер записывается в телеметрию как успех

`my-grpc-server-macros/src/generate_server/generate.rs:68`, `my-grpc-extensions/src/grpc_server_telemetry_context.rs:52-63`

Здесь `with_error` и слой телеметрии не стыкуются друг с другом.

`GrpcServerTelemetryContext::drop()` безусловно вызывает `write_success(..., "done", ...)`; `write_fail` существует в зафиксированной ревизии my-telemetry (`telemetry_interface.rs:90`) и не вызывается **нигде** во всём воркспейсе. До `with_error` это было терпимо: у unary-хендлера не было пути ошибки, кроме паники. Теперь `?` в `Ok(result?.into())` выходит рано, контекст дропается, событие пишется как успешное.

**Воспроизведено:** `generate_server!(..., with_telemetry: true, with_error: true)`, хендлер возвращает `Err(GrpcError::not_found("user not found"))`, запрос с метаданными `process-id: 42`. Вызывающая сторона получает `NOT_FOUND`; коллектор содержит ровно одно событие `data="GRPC: GetUser" success=Some("done") fail=None`. Доля ошибок на дашборде — постоянный 0%.

`README.md:166-168` эту особенность признаёт, но её стоит закрыть, а не документировать: правка локальная. `consts.rs` затеняет контекст (`let my_telemetry = my_telemetry.get_ctx();`), из-за чего в теле функции до объекта не добраться, — но `consts.rs` лежит в том же крейте, что и `generate.rs`.

**Как чинить:**

```rust
// consts.rs — перестать затенять
let my_telemetry_ctx = my_grpc_extensions::get_telemetry(..);
let my_telemetry = my_telemetry_ctx.get_ctx();

// generate.rs, ветка with_error
let result = #fn_name(&self.app, request.into(), my_telemetry).await;
if let Err(err) = &result {
    my_telemetry_ctx.set_error(err.to_string());
}
Ok(result?.into())
```

плюс `GrpcServerTelemetryContext::set_error(&self, msg: String)`, переключающий `Drop` на `write_fail`.

**Тот же провал у стримов, но хуже** (`generate.rs:134`): контекст дропается в момент возврата стрима — до отправки первого элемента и задолго до возможного `send_error`. Событие фиксирует длительность настройки хендлера, а не запроса. Так как `get_ctx()` отдаёт заимствование, обойти это на стороне вызова нельзя — гуард нужно перемещать внутрь возвращаемого стрима либо явно задокументировать, что для стримовых RPC замеряется только setup.

---

### 3. MEDIUM — тег `0.7.0` передвинут на новый API

`my-grpc-extensions/Cargo.toml:3`

`3d0a19c` поднял версию до `0.7.1`; `dd8ba19` откатил её обратно на `0.7.0` под сообщением, которое говорит только о макросах. При этом тег `0.7.0` указывает на `dd8ba19`:

```
git ls-remote --tags origin   →  dd8ba19a...  refs/tags/0.7.0
service-sdk/Cargo.lock:1263   →  tag=0.7.0#9f0f2d02c749601d5681f345a0abde16578ca4c4
```

Один тег — два материально разных API. В `9f0f2d0` нет ни `GrpcError`, ни `with_error`. Потребитель, собирающийся по локу service-sdk и следующий новому README, получит `E0432` на импорте `GrpcError`, а параметр `with_error` будет тихо проигнорирован (см. находку 6). Номер версии об этом не сигналит никак, и ни один downstream-репозиторий не может выразить «мне нужна сборка с `GrpcError`» — обе объявляют себя как `0.7.0`.

Паттерн не разовый: у тега `0.6.6` локально `be75a67`, на remote `452a697`, а в чужих локах встречается третья ревизия.

**Как чинить:** вернуть `version = "0.7.1"` в `my-grpc-extensions/Cargo.toml` (и бампнуть `my-grpc-server-macros` — он тоже получил параметр); вернуть тег `0.7.0` на `9f0f2d0`, чтобы уже зафиксированные потребители резолвились в то, против чего собирались; нарезать новый тег `0.7.1` на `1fed133` — в `dd8ba19` нет README-коммита, документирующего фичу. Релизные теги не двигать.

---

### 4. MEDIUM — `TransportError` теряет причину

`my-grpc-extensions/src/grpc_error.rs:80-82`

```rust
GrpcReadError::TransportError(err) => {
    Self(tonic::Status::unavailable(format!("Transport error: {}", err)))
}
```

У `tonic::transport::Error` реализация `Display` — это `f.write_str(self.description())`, а `Kind::Transport => "transport error"`. На выходе получается строка `"Transport error: transport error"`. Настоящая причина («tcp connect error, 127.0.0.1:1, Os { code: 61, ConnectionRefused }») живёт только в `Debug`/`source()`, а `GrpcReadError` поглощается матчем; `GrpcError` хранит лишь итоговый `Status` и не переопределяет `Error::source()`.

Вариант достижим на обычном пути запроса (`grpc_channel_holder.rs:246`, `Err(err.into())` после неудачного `end_point.connect()`), и на этом пути его никто не логирует: `drop_channel_if_needed` печатает `{:?}` только для `TonicStatus(Unknown)`, а `TransportError` попадает в `_ => false`. Остальной крейт логирует ошибки через `{:?}` (`grpc_channel_pool.rs:259`, `grpc_channel.rs:96`, `grpc_channel_holder.rs:75/206/220`) — эта строка выбивается из собственной конвенции.

**Как чинить:** наивная замена на `{:?}` утащит адрес и errno в статус, уходящий по проводу. Правильнее логировать `format!("Transport error: {:?}", err)` через `my_logger`, как крейт уже делает в других местах, а наружу отдавать `tonic::Status::unavailable("Upstream service is unavailable")`.

---

### 5. LOW — документация

**`README.md:162`.** Секция описывает флаг только как «unary handlers return `Result<TResponse, GrpcError>` instead of a bare `TResponse`». Но хендлер без ответа (`returns (google.protobuf.Empty)` либо без выходного параметра) идёт через ту же `unary_result_conversion` и тоже меняет сигнатуру — с «не возвращает ничего» на `Result<(), GrpcError>`. Собственный пример README `post_bid_ask` (`README.md:81` и `README.md:230`) ничего не возвращает, и при `with_error: true` сборка падает с `error[E0277]: the ? operator can only be applied to values that implement Try` в точке вызова макроса, без указания метода.

Добавить в Notes:

> A handler with no response (`returns (google.protobuf.Empty)`) also changes — it must return `Result<(), GrpcError>`. Only handlers returning `StreamedResponseWriter<T>` are exempt; a *streaming-input* method with an Empty response is not.

и переформулировать «Streamed responses are not affected» → «Streamed **output** responses are not affected» (сам по себе этот пункт верен: `PostBidAsk` стримит запрос, а не ответ).

**`my-grpc-extensions/src/grpc_error.rs:10-19`.** Rustdoc-пример объявляет только `app` и `request`, а в теле передаёт третьим аргументом `ctx`. Это не пользовательский плейсхолдер, а телеметрический контекст, который подставляет макрос (`generate.rs:153`), — значит хендлер обязан его объявить, ровно как в `README.md:141`. Два документа одной и той же фичи противоречат друг другу, а ```` ```ignore ```` гарантирует, что расхождение никогда не всплывёт в `cargo test --doc`. Добавить `ctx: &MyTelemetryContext,` в сигнатуру либо убрать `, ctx` из вызова.

---

### 6. LOW — опечатка в имени параметра макроса проглатывается молча

`my-grpc-server-macros/src/generate_server/generate.rs:54`

`generate_server!` не вызывает `TokensObject::check_for_unknown_params`, хотя зафиксированный types-reader 0.5.1 его предоставляет (`types-reader-core/src/tokens_object/tokens_object.rs:121`). Поиск по репозиторию даёт ноль вызовов.

`with_errors: true` вместо `with_error: true` → `try_get_named_param` возвращает `None` → флаг `false` → генерируется `Ok(result.into())` → хендлер, написанный как `-> Result<T, GrpcError>`, даёт `error[E0277]: the trait bound tonic::Response<T>: From<Result<T, GrpcError>> is not satisfied` в точке вызова макроса, без единого упоминания неизвестного параметра. Это ровно та же ошибка, которую получит потребитель с устаревшим локом из находки 3, — оттого её и тяжело диагностировать.

Соглашение общерепозиторное и предшествует этим коммитам (касается `grpc_struct_name`, `with_telemetry`, а в клиентских макросах — `overrides`/`service_name`), поэтому чинить логично единообразно:

```rust
params_list.check_for_unknown_params(&[
    "proto_file", "crate_ns", "grpc_struct_name", "with_telemetry", "with_error",
])?;
```

---

## Итог

Ядро — `GrpcError` и генерация `with_error` — сделано осмысленно и обратно совместимо; менять там нечего.

Единственная содержательная нестыковка внутри самого диффа: **телеметрия не знает про ошибки** (находка 2), и именно `with_error` впервые делает это регулярно достижимым. Плюс две вещи формально вне диффа, но требующие внимания первыми: удалённо триггерящаяся паника (находка 1) и передвинутый релизный тег (находка 3).

Приоритет: **1 → 3 → 2 → 4 → 6 → 5**.
