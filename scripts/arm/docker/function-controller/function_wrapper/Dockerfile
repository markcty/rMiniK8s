FROM python:3
WORKDIR /app
RUN pip config set global.index-url https://mirror.sjtu.edu.cn/pypi/web/simple && pip install flask
COPY . .
RUN pip install --no-cache-dir -r requirements.txt
EXPOSE 80
CMD ["python", "server.py"]
