#!/usr/bin/env node

/**
 * Tavily API 工具集
 *
 * 用法:
 *   node scripts/tavily.cjs search "query" [--depth advanced] [--results 8]
 *   node scripts/tavily.cjs extract <url> [url2 ...] [--depth advanced]
 *   node scripts/tavily.cjs crawl <url> [--depth 2] [--max-pages 20]
 *   node scripts/tavily.cjs map <url> [--max-depth 2] [--limit 30]
 *
 * API Key 存放在 scripts/.tavily-key（已 gitignore），一行纯文本
 */

const fs = require('fs');
const path = require('path');

const KEY_FILE = path.join(__dirname, '.tavily-key');
const BASE = 'https://api.tavily.com';

function getApiKey() {
  if (process.env.TAVILY_API_KEY) return process.env.TAVILY_API_KEY;
  if (fs.existsSync(KEY_FILE)) return fs.readFileSync(KEY_FILE, 'utf-8').trim();
  console.error('No API key found. Set TAVILY_API_KEY env or create scripts/.tavily-key');
  process.exit(1);
}

function takeFlag(args, flag, fallback) {
  const i = args.indexOf(flag);
  if (i === -1) return fallback;
  const val = args[i + 1];
  args.splice(i, 2);
  return val ?? fallback;
}

async function apiCall(endpoint, body) {
  const key = getApiKey();
  const res = await fetch(`${BASE}/${endpoint}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${key}`,
    },
    body: JSON.stringify({ api_key: key, ...body }),
  });
  if (!res.ok) {
    const text = await res.text();
    console.error(`Tavily ${endpoint} error: ${res.status} ${text}`);
    process.exit(1);
  }
  return res.json();
}

// ── search ───────────────────────────────────────────────────────────
async function cmdSearch(argv) {
  const args = [...argv];
  const depth = takeFlag(args, '--depth', 'basic');
  const maxResults = parseInt(takeFlag(args, '--results', '5'), 10);
  const noAnswer = args.includes('--no-answer');
  args.splice(args.indexOf('--no-answer'), 1);
  const query = args.filter(a => !a.startsWith('--'))[0];

  if (!query) {
    console.error('Usage: tavily search "query" [--depth basic|advanced] [--results N]');
    process.exit(1);
  }

  const data = await apiCall('search', {
    query, search_depth: depth, max_results: maxResults, include_answer: !noAnswer,
  });

  if (data.answer) { console.log('=== Answer ===\n' + data.answer + '\n'); }
  console.log(`=== Results (${data.results.length}) ===`);
  for (const r of data.results) {
    console.log(`\n[${r.score?.toFixed(2) ?? '?'}] ${r.title}\n  ${r.url}`);
    if (r.content) console.log(`  ${r.content.replace(/\n/g, ' ').slice(0, 300)}${r.content.length > 300 ? '...' : ''}`);
  }
  if (data.follow_up_questions?.length) {
    console.log('\n=== Follow-up ===');
    data.follow_up_questions.forEach(q => console.log(`  - ${q}`));
  }
}

// ── extract ──────────────────────────────────────────────────────────
async function cmdExtract(argv) {
  const args = [...argv];
  const depth = takeFlag(args, '--depth', 'basic');
  const urls = args.filter(a => !a.startsWith('--'));

  if (!urls.length) {
    console.error('Usage: tavily extract <url> [url2 ...] [--depth basic|advanced]');
    process.exit(1);
  }

  const data = await apiCall('extract', { urls: urls.length === 1 ? urls[0] : urls, extract_depth: depth });

  for (const r of data.results || []) {
    console.log(`\n=== ${r.url} ===`);
    console.log(r.raw_content?.slice(0, 5000) ?? '(empty)');
  }
  if (data.failed_results?.length) {
    console.log('\n=== Failed ===');
    data.failed_results.forEach(f => console.log(`  ${f.url}: ${f.error}`));
  }
  console.log(`\n(${data.response_time?.toFixed(1) ?? '?'}s)`);
}

// ── crawl ────────────────────────────────────────────────────────────
async function cmdCrawl(argv) {
  const args = [...argv];
  const maxDepth = parseInt(takeFlag(args, '--max-depth', '2'), 10);
  const maxPages = parseInt(takeFlag(args, '--max-pages', '20'), 10);
  const selectPaths = takeFlag(args, '--paths', '');
  const url = args.filter(a => !a.startsWith('--'))[0];

  if (!url) {
    console.error('Usage: tavily crawl <url> [--max-depth 2] [--max-pages 20] [--paths "/docs,/api"]');
    process.exit(1);
  }

  const body = { url, max_depth: maxDepth, max_pages: maxPages };
  if (selectPaths) body.select_paths = selectPaths.split(',').map(s => s.trim());

  const data = await apiCall('crawl', body);

  console.log(`=== Crawled ${(data.results || []).length} pages ===`);
  for (const r of data.results || []) {
    console.log(`\n--- ${r.url} ---`);
    const content = r.raw_content || r.content || '';
    console.log(content.slice(0, 3000) + (content.length > 3000 ? '\n...(truncated)' : ''));
  }
  if (data.response_time) console.log(`\n(${data.response_time.toFixed(1)}s)`);
}

// ── map ──────────────────────────────────────────────────────────────
async function cmdMap(argv) {
  const args = [...argv];
  const maxDepth = parseInt(takeFlag(args, '--max-depth', '2'), 10);
  const limit = parseInt(takeFlag(args, '--limit', '30'), 10);
  const selectPaths = takeFlag(args, '--paths', '');
  const url = args.filter(a => !a.startsWith('--'))[0];

  if (!url) {
    console.error('Usage: tavily map <url> [--max-depth 2] [--limit 30] [--paths "/docs,/api"]');
    process.exit(1);
  }

  const body = { url, max_depth: maxDepth, limit };
  if (selectPaths) body.select_paths = selectPaths.split(',').map(s => s.trim());

  const data = await apiCall('map', body);

  console.log(`=== Site Map (${(data.results || []).length} URLs) ===`);
  for (const r of data.results || []) {
    console.log(typeof r === 'string' ? r : r.url || JSON.stringify(r));
  }
  if (data.response_time) console.log(`\n(${data.response_time.toFixed(1)}s)`);
}

// ── main ─────────────────────────────────────────────────────────────
const commands = { search: cmdSearch, extract: cmdExtract, crawl: cmdCrawl, map: cmdMap };
const [cmd, ...rest] = process.argv.slice(2);

if (!cmd || !commands[cmd]) {
  console.log('Usage: tavily <search|extract|crawl|map> [args]');
  console.log('  search  "query"  [--depth advanced] [--results 5]');
  console.log('  extract <url>    [--depth basic|advanced]');
  console.log('  crawl   <url>    [--max-depth 2] [--max-pages 20]');
  console.log('  map     <url>    [--max-depth 2] [--limit 30]');
  process.exit(cmd ? 1 : 0);
}

commands[cmd](rest).catch(e => { console.error(e); process.exit(1); });
