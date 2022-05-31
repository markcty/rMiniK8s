def handler(args):
    a = args.get("ans")
    return {"ans": a-3}


args = {"ans": 13}
print(handler(args))
