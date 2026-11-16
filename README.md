# Harmony

Discord-like chat app: Rust (Rocket) backend, React frontend, WebRTC SFU for calls.

## Demo

Needs Docker Desktop running. The first run builds everything and takes a few minutes.

```sh
./demo.sh             # friend on the same Wi-Fi: open the printed https://<your LAN IP>
./demo.sh --internet  # friend anywhere: prints a https://*.trycloudflare.com link
./demo.sh logs        # tail logs
./demo.sh down        # stop (keeps data; wipe it with: docker compose -f docker-compose.demo.yml down -v)
```

- Same Wi-Fi: the cert is self-signed, so on first visit click Advanced -> Proceed. Camera/mic need the https URL.
- Internet: for voice/video, forward UDP ports 50000-50199 on your router to your Mac's LAN IP (the script prints it). No TURN server is included. Calls on the same Wi-Fi need `./demo.sh` (LAN mode).
