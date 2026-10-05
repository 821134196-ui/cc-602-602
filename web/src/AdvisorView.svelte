<script>
  import { onMount } from 'svelte'
  import { api, statusLabel, fmtTime } from './api.js'

  let theses = $state([])
  let detail = $state(null)
  let error = $state('')
  let msg = $state('')

  async function load() {
    const data = await api('/api/theses')
    theses = data.theses
    if (detail) detail = await api(`/api/theses/${detail.id}`)
  }
  async function select(id) {
    error = ''
    detail = await api(`/api/theses/${id}`)
  }
  async function confirm() {
    error = ''; msg = ''
    try {
      await api(`/api/theses/${detail.id}/confirm`, { method: 'POST', body: {} })
      msg = '已确认送审，等待秘书进行匿名检查'
      await load()
    } catch (e) { error = e.message }
  }
  onMount(load)
</script>

<div class="card">
  <h2>我指导的论文</h2>
  {#if theses.length === 0}<p class="muted">暂无学生论文。</p>{/if}
  <table>
    <thead><tr><th>题目</th><th>学生</th><th>状态</th><th></th></tr></thead>
    <tbody>
      {#each theses as t}
        <tr>
          <td>{t.title}</td>
          <td>{t.student_name}</td>
          <td>{statusLabel(t.status)}</td>
          <td><button class="ghost" onclick={() => select(t.id)}>查看</button></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

{#if detail}
  <div class="card">
    <h2>{detail.title} <span class="badge">{statusLabel(detail.status)}</span></h2>
    <p class="muted">学生：{detail.student_name}</p>

    {#if detail.versions.length}
      {@const v = detail.versions[0]}
      <p>最新稿：第 {v.seq} 版 · {v.orig_filename}（{fmtTime(v.uploaded_at)}）
        <a href={`/api/files/${v.id}/download`} download>下载</a></p>
    {:else}
      <p class="muted">学生尚未上传稿件。</p>
    {/if}

    {#if detail.status === 'draft' && detail.versions.length}
      <button onclick={confirm}>确认送审</button>
    {/if}
    {#if msg}<p class="ok">{msg}</p>{/if}
    {#if error}<p class="error">{error}</p>{/if}

    {#each detail.rounds as r}
      <div class="round">
        <h4>第 {r.round_no} 轮送审 <span class="badge">{statusLabel(r.status)}</span>
          <span class="muted">送审稿：第 {r.version_seq} 版</span></h4>
        {#if r.assignment_count > 0}
          <p>评阅进度：{r.submitted_count} / {r.assignment_count} 已提交（评阅人身份对导师保密）</p>
        {/if}
        {#if r.status === 'released'}
          {#each r.assignments as a}
            {#if a.report}
              <div class="report">
                <strong>{a.alias}</strong>
                {#if a.report.score !== null}<span class="badge">{a.report.score} 分</span>{/if}
                {#if a.report.recommendation}<span class="badge">{a.report.recommendation}</span>{/if}
                <p>{a.report.comment}</p>
              </div>
            {/if}
          {/each}
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .round { border-top: 1px solid #eee; padding-top: 10px; margin-top: 10px; }
  .report { background: #f7f9fc; border-radius: 8px; padding: 10px 14px; margin: 8px 0; }
  a { color: #24509e; }
</style>
