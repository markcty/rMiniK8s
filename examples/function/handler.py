import time


def handler(args):
    startTime = time.time()
    time.sleep(0.1)
    endTime = time.time()
    return str(round((endTime - startTime) * 1000)) + "ms"
