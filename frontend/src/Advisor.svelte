<script>
  import { onMount } from 'svelte'
  import { api, downloadFile } from './api.js'
  import { STATUS_TEXT, REC_TEXT, fmtTime } from './text.js'

  let theses = []
  let error = ''
  let notice = ''

  async function load() {
    theses = await api('/api/advisor/theses')
  }

  onMount(async () => {
    try { await load() } catch (e) { error = e.message }
  })

  async function confirm(thesisId) {
    error = ''; notice = ''
    try {
      await api(`/api/advisor/theses/${thesisId}/confirm`, { method: 'POST' })
      notice = '已确认，等待秘书进行匿名审查'
      await load()
    } catch (e) { error = e.message }
  }

  async function download(versionId, filename) {
    error = ''
    try { await downloadFile(versionId, filename) } catch (e) { error = e.message }
  }
</script>

{#if error}<div class="error">{error}</div>{/if}
{#if notice}<div class="ok">{notice}</div>{/if}

{#each theses as t}
  <div class="card">
    <h2>
      《{t.title}》
      <span class="badge status">{STATUS_TEXT[t.status] || t.status}</span>
    </h2>
    <div class="muted">学生：{t.student_name} ｜ 创建时间：{fmtTime(t.created_at)}</div>

    {#if t.status === 'uploaded'}
      <div style="margin:10px 0">
        <button on:click={() => confirm(t.id)}>确认稿件可送审</button>
      </div>
    {/if}

    <h3>稿件版本</h3>
    <table>
      <thead>
        <tr><th>版本</th><th>文件名</th><th>上传时间</th><th>标记</th><th></th></tr>
      </thead>
      <tbody>
        {#each t.versions as v}
          <tr>
            <td>第 {v.version_no} 版</td>
            <td>{v.filename}</td>
            <td>{fmtTime(v.uploaded_at)}</td>
            <td>
              {#if v.id === t.latest_version_id}<span class="badge latest">最新上传稿</span>{/if}
              {#if v.is_review_version}<span class="badge review">实际送审稿</span>{/if}
            </td>
            <td><button class="ghost" on:click={() => download(v.id, v.filename)}>下载</button></td>
          </tr>
        {/each}
        {#if t.versions.length === 0}
          <tr><td colspan="5" class="muted">学生尚未上传稿件</td></tr>
        {/if}
      </tbody>
    </table>

    <h3>评阅意见（匿名）</h3>
    {#if t.reviews_released}
      {#each t.reviews as r}
        <div class="review-box">
          <div><strong>{r.label}</strong>
            <span class="badge">{REC_TEXT[r.recommendation] || r.recommendation}</span>
            {#if r.score != null}<span class="badge">{r.score} 分</span>{/if}
          </div>
          <div style="margin-top:6px">{r.comment}</div>
        </div>
      {/each}
    {:else}
      <div class="muted">意见尚未开放。评阅人身份对您不可见。</div>
    {/if}
  </div>
{/each}

{#if theses.length === 0}
  <div class="card muted">暂无您指导的论文。</div>
{/if}
