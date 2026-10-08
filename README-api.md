1. 获取所有图书
GET baseUrl/getBookshelf
传参：无
返回：
durChapterIndex 为正在阅读的章节索引
durChapterPos 为正在阅读的正文位置
latestChapterTitle 为已经更新到的章节标题
```json
{
  "data": [
    {
      "author": "刘慈欣",
      "bookUrl": "https://example.org/book/39020.htm",
      "coverUrl": "https://example.org/files/article/image/39/39020/39020s.jpg",
      "durChapterIndex": 10,
      "durChapterPos": 0,
      "durChapterTitle": "6.射手和农场主",
      "intro": "文化大革命如火如荼进行的同时。军方探寻外星文明的绝秘计划红岸工程取得了突破性进展。但在按下发射键的那一刻，历经劫难的叶文洁没有意识到，她彻底改变了人类的命运。 地球文明向宇宙发出的第一声啼鸣，以太阳为中心，以光速向宇宙深处飞驰…… 四光年外，三体文明正苦苦挣扎——三颗无规则运行的太阳主导下的百余次毁灭与重生逼迫他们逃离母星。而恰在此时。他们接收到了地球发来的信息。在运用超技术锁死地球人的基础科学之后。三体人庞大的宇宙舰队开始向地球进发……人类的末日悄然来临。",
      "kind": "都市小说,连载,2022-08-11",
      "lastCheckCount": 0,
      "lastCheckTime": 1791333091673,
      "latestChapterTime": 1791333091673,
      "latestChapterTitle": "35.尾声。遗址",
      "name": "三体",
      "order": -31,
      "originOrder": 1,
      "totalChapterNum": 35,
      "wordCount": "133.46万字"
    },
    {
      "author": "刘慈欣",
      "bookUrl": "https://example.org/book/25234.htm",
      "coverUrl": "https://example.org/files/article/image/25/25234/25234s.jpg",
      "durChapterIndex": 0,
      "durChapterPos": 0,
      "durChapterTitle": "第三章",
      "intro": "三体人在利用魔法般的科技锁死了地球人的科学之后，庞大的宇宙舰队杀气腾腾地直扑太阳系，意欲清除地球文明。\n面对前所未有的危局，经历过无数磨难的地球人组建起同样庞大的太空舰队，同时，利用三体人思维透明的致命缺陷，制订了神秘莫测的“面壁计划”，精选出四位“面壁者”。秘密展开对三体人的反击。\n三体人自身虽然无法识破人类的诡谲计谋，却依靠由地球人中的背叛者挑选出的“破壁人”，与“面壁者”展开智慧博弈……\n“面壁计划”究竟能否成功？地球人究竟能否在这场你死我活的文明生存竞争中战而胜之？神秘的\n“黑暗森林”究竟意味着什么？",
      "kind": "游戏竞技,全本,2017-12-18",
      "lastCheckCount": 0,
      "lastCheckTime": 1787400303196,
      "latestChapterTime": 1787400303196,
      "latestChapterTitle": "第十九章",
      "name": "三体Ⅱ·黑暗森林",
      "order": -19,
      "totalChapterNum": 913,
      "wordCount": "570.68万字"
    }
  ],
  "errorMsg": "",
  "isSuccess": true
}
```
2. 获取一本书的所有章节
GET baseUrl/getChapterList?url=%bookUrl%
传参： url 一本图书的 bookUrl
响应：
```json
{
  "data": [
    {
      "baseUrl": "https://example.org/book/39020/",
      "bookUrl": "https://example.org/book/39020.htm",
      "title": "第1章 身份",
      "url": "https://example.org/txt/39020/26690634"
    },
    {
      "baseUrl": "https://example.org/book/39020/",
      "bookUrl": "https://example.org/book/39020.htm",
      "title": "第2章",
      "url": "https://example.org/txt/39020/26690635"
    }
  ],
  "errorMsg": "",
  "isSuccess": true
}
```
3. 获取正文
GET baseUrl/getBookContent?url=https%3A%2F%2Fexample.org%2Fbook%2F39020.htm&index=77
传参： 
url 一本图书的 bookUrl
index 章节index
响应：
```json
{
  "data": "　　危机纪年第205年，三体舰队距太阳系2。10光年黑暗出现了，这之前连黑暗都没有，只有虚无。虚无是无色彩的，虚无什么都没有，有黑暗，至少意味着出现了空间。很快，黑暗的空间中出现了一些扰动，像穿透一切的微风，这是时间流逝的感觉。之前的虚无是没有时间的，现在时间也出现了，像消融的冰河。光的出现是在很长时间以后，开始，只是一片没有形状的亮斑，又经过了很漫长的等待，世界的形状才显现出来。刚刚复活的意识在努力分辨着，最初看清的是几根横空而过的透明细管，然后是管道后面的一张俯视着的人脸，人脸很快消失，露出发着乳白色光芒的天花板。",
  "errorMsg": "",
  "isSuccess": true
}
```
4. 保存图书进度
POST baseUrl/saveBookProgress
传参：
```json
{
  "name": "三体",
  "author": "刘慈欣",
  "durChapterIndex": 77,
  "durChapterPos": 0,
  "durChapterTime": 1791425061524,
  "durChapterTitle": "第78章"
}
```
响应：
```json
{
  "data": "",
  "errorMsg": "",
  "isSuccess": true
}
```