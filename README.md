# koin

### Automatic backups (cron sidecar)
This project includes a lightweight backup sidecar service (`pg-backup`) in `compose.yaml` that runs scheduled `pg_dump` jobs and writes binary dumps to the backup folder.

### Restore backup
```bash
./scripts/restore_db_from_dump.sh /absolute_path_to/postgres_koin_backup_20260119_214458.dump
```
