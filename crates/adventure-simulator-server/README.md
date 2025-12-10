# Adventure Simulator Server

Server binary for the adventure simulator project.

## Configuration

Though there are default values for the server configuration, it can be manually configured in two ways (in priority order):

### CLI arguments: `--%arg%`

Use `--help` to see available arguments.

### Enviroment variables 

You can also pass cli arguments as enviromental variables, e.g. `ADVENTURE_SIMULATOR_ADDR="127.0.0.1:5100"`

### Config file: *-c config.toml*

By running a server with `-c` /  `--config` option, you can pass a toml file with configuration:

```toml
addr = "127.0.0.1:4000"
enable_websocket = true
enable_udp = true
protocol_id = 0
protocol_private_key = "0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0"
```
