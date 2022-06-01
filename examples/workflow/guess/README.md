# Guess Workflow

1. create function: guess, minus3, right, wrong
2. create workflow: guess

Try it out:

* `http -v GET guess.workflow.func.minik8s.com a:=9 b:=4`: You should be able to get "Congratulations! You are right!" in the end.
* `http -v GET guess.workflow.func.minik8s.com a:=1 b:=2`: You should be able to get "Sorry... Guess again..." in the end.
