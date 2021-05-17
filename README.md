
## Run

cargo run -- --chain=$HOME/dot.json -d /tmp/dot --execution=wasm



## dependency:

- client/archive/postgres

```text
#sqlx = { version = "0.5", features = ["postgres", "runtime-tokio-rustls", "json"] }
#sqlx = { version = "0.5", features = ["postgres", "runtime-tokio-native-tls", "json"] }
sqlx = { version = "0.5", features = ["postgres", "runtime-async-std-native-tls", "json"] }
```

- client/archive/kafka

```text
replace sp-core="3.0.0" to github
```
