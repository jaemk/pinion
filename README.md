> what's your pinion

### requirements

```shell
postgres # https://postgresapp.com/
rust     # https://rustup.rs/
migrant  # cargo install migrant --features postgres
```

### setup

```shell
cp .env.sample .env
createuser pinion -P
createdb -O pinion pinion
migrant setup
migrant apply -a
```

### run

```shell
cargo run
```
