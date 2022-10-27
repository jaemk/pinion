> track that poop

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

### build

```shell
./docker.sh build
```
