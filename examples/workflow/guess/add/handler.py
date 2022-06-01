def handler(args):
    a = args.get("a")
    b = args.get("b")
    return {"ans": a+b}


args = {"a": 3, "b": 4}
print(handler(args))
