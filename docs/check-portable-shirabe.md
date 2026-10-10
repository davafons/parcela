# Check a paused Shirabe replica on Linux

1. Prepare a private, user-owned destination test directory. Keep it separate from existing application data and services.
2. Supply a source-only jj checkpoint of Shirabe that includes `bin/portable-uploads`.
3. Supply finalized SQLite snapshots named `primary.sqlite3`, `dictionary.sqlite3`, `queue.sqlite3`, and `cache.sqlite3`. Reject live files with WAL, SHM, or journal sidecars.
4. Capture uploads with Shirabe's `bin/portable-uploads capture`, using the captured primary database rather than the current live database.
5. Transfer the source checkpoint and upload snapshot into the private destination directory.
6. Install the captured project's Ruby dependencies into a task-local `BUNDLE_PATH`. Set `BUNDLE_FROZEN=true` and preserve `Gemfile.lock`.
7. Install the project's pinned Sudachi **full** edition in a private tokenizer directory. This source pins version `20260723` in `lib/tasks/dictionary.rake` and `config/allgood.rb`. Do not run the broad dictionary import or database preparation recipe against the copied data.

   ```sh
   KABOSU_DICT_DIR=/private/test/tokenizer \
   BUNDLE_PATH=/private/test/gems BUNDLE_FROZEN=true \
   bundle exec ruby -rkabosu -e \
     'Kabosu::DictManager.new.install_if_missing("full", version: ARGV.fetch(0))' \
     20260723
   ```

   Retain the upstream license and record the installed file's SHA-256. Verify downloaded archives against the upstream release checksum. Supply a tokenizer directory containing only the selected full-edition dictionary.

8. Supply explicit paths to Parcela, Caddy 2.x, and Process Compose 1.x. Put lsof and Ruby on the test's `PATH`.
9. Create a short, private state parent on a disk-backed filesystem, such as `/home/USER/.parcela-tests`. Leave space for independent database copies and uploads. Avoid quota-limited `/tmp`.
10. Choose two unused loopback test ports above 1023.
11. Run the check on the destination host:

   ```sh
   BUNDLE_PATH=/private/test/gems \
   BUNDLE_APP_CONFIG=/private/test/bundle-config \
   BUNDLE_USER_HOME=/private/test/bundle-home \
   python3 scripts/check_portable_shirabe.py \
     --parcela /private/tools/parcela \
     --checkpoint /private/test/shirabe-code-checkpoint \
     --database-dir /private/test/database-snapshots \
     --uploads-snapshot /private/test/uploads-snapshot \
     --tokenizer-dir /private/test/tokenizer --tokenizer-version 20260723 \
     --work-dir /private/test/new-acceptance-run \
     --state-parent /home/USER/.parcela-tests \
     --caddy /private/tools/caddy \
     --process-compose /private/tools/process-compose \
     --http-port 18180 --https-port 18443
   ```

12. Inspect `new-acceptance-run/report.json` and the retained child logs. Require `complete: true` before recording acceptance.
13. Retain the input snapshots. Keep failed runs for diagnosis instead of retrying into the same output directory.

For the exact check boundaries and receipt fields, see [transfer commands](transfer.md#destination-shirabe-acceptance). This check does not activate an agent or transfer execution ownership.
