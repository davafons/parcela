import time

tick = 0
while True:
    tick += 1
    print(f"tick {tick}", flush=True)
    time.sleep(5)
