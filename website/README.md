# Cditor Website

零依赖静态官网。

```bash
python3 -m http.server 4173 --directory website
```

然后访问 `http://localhost:4173`。

运行页面结构与交互测试：

```bash
node --test website/site.test.mjs
```
