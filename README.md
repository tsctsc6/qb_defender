# qb_defender

A small tool to help qBittorrent(qBittorrent -Enhanced-Edition is included) block Xunlei and other leeching BT clients.

## Working principle

Obtain information through WebUI of qBittorrent at regular intervals, and illegal peers can be identified using rules below:

1. Client name is Leech client or Ancient client. For details, please see src/application.rs
1. Total upload exceeds reported progress * torrent size + 10 MB
1. Progress is regressive more than 10MB/TorrentSize
1. Uploaded is regressive more than 10MB
1. The banned IPs will reset after running continuously for more than one day.

## Usage

1. Enable WebUI of qBittorrent, and remember the port. (Authentication is not supported)
1. Execute command: `qb_defender --port <port>`

> [!TIP]
> You can start qb_defender as a daemon, like Windows Service, etc.

> [!TIP]
> Recommended to use in combination with [BT_BAN](https://github.com/Oniicyan/BT_BAN).

### How to confirm that it is working

You can check the banned IPs by visit "http://127.0.0.1:port/api/v2/app/preferences".
