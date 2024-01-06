# kill-orphan

Small utility which allows to run a command which will be killed as soon as the parent process dies.

Use-case: e.g. running a watcher task which rebuilds assets as long as some main application is running.

## Usage

```sh 
kill-orphan command [args...]
```