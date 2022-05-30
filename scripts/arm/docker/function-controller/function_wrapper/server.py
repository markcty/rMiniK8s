from flask import Flask, request
from handler import handler

app = Flask(__name__)


@app.route("/")
def function():
    print("received request")
    args = {}
    if request.data:
        args = request.get_json()
    print(args)

    return handler(args)


app.run(host='0.0.0.0', port=80)
