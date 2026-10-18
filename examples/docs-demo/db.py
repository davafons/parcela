import os
import sqlite3
import sys

database = sqlite3.connect(os.environ["DATABASE_PATH"])

if sys.argv[1] == "setup":
    database.execute("CREATE TABLE IF NOT EXISTS messages (text TEXT)")
    database.execute("INSERT INTO messages VALUES (?)", (f"hello from {os.environ['PARCELA_ENVIRONMENT_NAME']}",))
    database.commit()
    print("database ready")
else:
    for (text,) in database.execute("SELECT text FROM messages"):
        print(text)
