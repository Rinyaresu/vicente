## README

### Overview

This project consists of a Rust server that serves an RSS feed and an Astro frontend that displays the articles from this feed. The server uses Actix Web to serve the data.

### How to Run the Project

#### Rust Server

1. **Run the Rust server:**

```sh
cargo run
```

2. **Make a request to the server to get the articles:**

```sh
curl http://127.0.0.1:8080/articles
```

#### Astro Frontend

1. **Astro file structure:**

```astro
---
import Layout from "@layouts/Layout.astro";
---

<Layout>
  <main id="hero" style="padding: 20px; display: flex; flex-direction: column;">
    <h1
      style="color: darkslateblue; margin: 1rem 0; display: inline-block; font-size: 1.875rem; font-weight: 700; margin: 2rem 0; font-size: 3rem;"
    >
      RSS Feed
    </h1>
    <button
      id="fetchButton"
      style="margin-bottom: 1.25rem; width: 12rem; cursor: pointer; border-radius: 0.25rem; border: none; background-color: #3b82f6; padding: 0.625rem 1.25rem; color: white;"
    >
      Fetch Articles
    </button>
    <p id="lastFetched" style="margin-bottom: 20px;"></p>
    <ul class="blog-names" style="list-style: none; padding: 0;"></ul>
    <ul class="articles" style="list-style: none; padding: 0;"></ul>
    <p id="requestFinished" style="margin-top: 20px;"></p>
  </main>
</Layout>

<script>
  interface Article {
    title: string;
    link: string;
    description: string;
    pub_date: string;
    feed_title: string;
    content_encoded: string;
  }

  let articles: Article[] = [];
  let blogNames: string[] = [];
  let selectedBlog: string | null = null;
  let error: string | null = null;
  let lastFetched: string | null = null;
  let requestFinished: boolean = false;

  window.handleFetchArticles = async () => {
    try {
      console.log("Button clicked to fetch articles");
      const response = await fetch("http://192.168.1.91:8080/articles", {
        headers: {
          "Content-Type": "application/json",
        },
      });
      if (!response.ok) {
        throw new Error("Failed to fetch articles");
      }
      articles = await response.json();
      console.log("Fetched articles:", articles);
      localStorage.setItem("articles", JSON.stringify(articles));
      blogNames = [...new Set(articles.map(article => article.feed_title))];
      error = null;
      updateBlogNames();

      lastFetched = new Date().toLocaleString();
      localStorage.setItem("lastFetched", lastFetched);
      updateLastFetched();
    } catch (err) {
      console.error("Error fetching articles:", err);
      error = err.message;
      updateError();
    } finally {
      requestFinished = true;
      updateRequestFinished();
    }
  };

  window.loadArticlesFromLocalStorage = () => {
    const storedArticles = localStorage.getItem("articles");
    if (storedArticles) {
      articles = JSON.parse(storedArticles);
      console.log("Loaded articles from localStorage:", articles);
      blogNames = [...new Set(articles.map(article => article.feed_title))];
      updateBlogNames();
    }
    lastFetched = localStorage.getItem("lastFetched");
    updateLastFetched();
  };

  window.selectBlog = blogName => {
    selectedBlog = blogName;
    updateArticles();
  };

  const updateBlogNames = () => {
    const ul = document.querySelector("ul.blog-names");
    if (ul) {
      ul.innerHTML = blogNames
        .map(
          blogName => `
        <li key="${blogName}" style="margin-bottom: 10px; color: blue;">
          <a href="javascript:void(0)" onclick="selectBlog('${blogName.replace(/'/g, "\\'")}')" style="color: darkblue;">${blogName}</a>
        </li>
      `
        )
        .join("");
    }
  };

  const updateArticles = () => {
    const ul = document.querySelector("ul.articles");
    if (ul) {
      ul.innerHTML = articles
        .filter(article => article.feed_title === selectedBlog)
        .map(
          (article, index) => `
          <li key="${index}" style="margin-bottom: 20px; padding: 10px; border-bottom: 1px solid #ccc; word-wrap: break-word; max-width: 800px;">
            <p><strong style="color: darkred;">${article.feed_title}</strong></p>
            <p><a href="javascript:void(0)" onclick="showContent(${index})" style="font-size: 18px; font-weight: bold; color: green; word-wrap: break-word;">${article.title}</a></p>
            <p style="color: gray;">${article.pub_date}</p>
            <div id="content-${index}" style="display: none; word-wrap: break-word;">
              <p style="word-wrap: break-word;">${article.description}</p>
              <pre style="background: #f4f4f4; padding: 10px; word-wrap: break-word;">${article.content_encoded}</pre>
            </div>
          </li>
        `
        )
        .join("");
    }
  };

  window.showContent = index => {
    const contentDiv = document.getElementById(`content-${index}`);
    if (contentDiv) {
      contentDiv.style.display =
        contentDiv.style.display === "none" ? "block" : "none";
    }
  };

  const updateError = () => {
    const main = document.querySelector("main");
    if (main) {
      main.innerHTML += `<p id="error" style="color: red;">Error: ${error}</p>`;
    }
  };

  const updateLastFetched = () => {
    const lastFetchedElem = document.getElementById("lastFetched");
    if (lastFetchedElem && lastFetched) {
      lastFetchedElem.textContent = `Last fetched: ${lastFetched}`;
      lastFetchedElem.style.color = "purple";
    }
  };

  const updateRequestFinished = () => {
    const requestFinishedElem = document.getElementById("requestFinished");
    if (requestFinishedElem) {
      requestFinishedElem.textContent = requestFinished
        ? "Request completed."
        : "";
      requestFinishedElem.style.color = "green";
    }
  };

  document
    .getElementById("fetchButton")
    .addEventListener("click", handleFetchArticles);
  document.addEventListener("DOMContentLoaded", loadArticlesFromLocalStorage);
</script>
```

## TODO

- [ ] List of recently read articles
- [ ] Mark an article as read
- [ ] Remove articles from the list once read
- [ ] Display the date and time when an article is read
