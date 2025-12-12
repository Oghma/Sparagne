# Sparagne

A budget tracker for personal finance and budget planning. Currently, the
application is an **early alpha**.

Sparagne (in italian "risparmiare") is a furlan word that means "savings".

The app consists of:
- an engine that manages expenses, cash flows, wallets, etc
- a server that exposes the API
- a telegram bot
- in future a TUI interface

## Installation

### Option 1: From Docker

Pull the image from the docker hub

``` sh
docker pull oghma/sparagne
```

Open `settings.toml` and change the settings. See [Settings](#Settings). Save
the settings and run the docker with

``` sh
docker run -dit -v ./path to settings folder:/sparagne/config oghma/sparagne
```


### Option 2: From Source

Clone the `sparagne` respository and navigate to the root directory

``` sh
git clone git@github.com:Oghma/Sparagne.git
cd sparagne
```

Open `config/config.toml` and change the settings. See [Settings](#Settings).
Save the settings and run `Sparagne`

``` sh
cargo run -p sparagne --release
```

### Database

Sparagne requires a database to store users and their entries. At the moment
only `Sqlite3` is supported. There is no utility to create users and their
vaults so you have to create them manually inside the database.

NOTE: Telegram bot requires its account for the authentication.

## Settings

`server.database` is the path to the sqlite3 database

To use the telegram bot `[telegram]` settings need to have enabled
- `token`: Telegram token
- `server`: ip address of the sparagne server. For now is hardcoded to `"http://127.0.0.1:3000"`
- `username`: username of the telegram database account. See [Database](#Database)
- `password`: password of the telegram database account

