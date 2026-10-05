<script>
  import { onMount } from 'svelte'
  import { api, statusLabel, fmtTime } from './api.js'

  let assignments = $state([])
  let error = $state('')
  let msg = $state('')
  let forms = $state({})

  async function load() {
    const list = (await api('/api/reviewer/assignments')).assignments
    const f = { ...forms }
    for (const a of list) f[a.assignment_id] ??= { score: '', recommendation: '', comment: '' }
    forms = f
    assignments = list
  }
  async function submit(a) {
    error = ''; msg = ''
    const f = forms[a.assignment_id] || {}
    try {
      await api(`/api/assignments/${a.assignment_id}/report`, {
        method: 'POST',
        body: { score: f.score ? Number(f.score) : null, recommendation: f.recommendation || null, comment: f.comment || '' },
      })
      msg = '评阅意见已提交'
      await load()
    } catch (e) { error = e.message }
  }

  onMount(load)
</script>

<div class="card">
  <h2>我的评阅任务</h2>
  <p class="muted">仅能看到分配给自己的稿件；稿件作者信息已匿名化处理。</p>
  {#if msg}<p class="ok">{msg}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
  {#if assignments.length === 0}<p class="muted">暂无评阅任务。</p>{/if}

  {#each assignments as a}
    <div class="task">
      <h3>评阅任务 #{a.assignment_id}（第 {a.round_no} 轮）<span class="badge">{statusLabel(a.round_status)}</span></h3>
      <p>论文题目：{a.title}</p>
      <p>稿件：第 {a.version_seq} 版 · {a.filename}
        <a href={`/api/files/${a.version_id}/download`} download>下载稿件</a></p>
      <p class="muted">分配时间：{fmtTime(a.assigned_at)}</p>

      {#if a.submitted}
        <p class="ok">评阅意见已提交。</p>
      {:else if a.round_status === 'in_review'}
        <div class="form">
          <label>评分（0-100）
            <input type="number" min="0" max="100" bind:value={forms[a.assignment_id].score} />
          </label>
          <label>结论建议
            <select bind:value={forms[a.assignment_id].recommendation}>
              <option value="">请选择</option>
              <option>同意答辩</option>
              <option>修改后答辩</option>
              <option>不同意答辩</option>
            </select>
          </label>
          <label>评阅意见
            <textarea rows="4" bind:value={forms[a.assignment_id].comment}></textarea>
          </label>
          <button onclick={() => submit(a)}>提交评阅意见</button>
        </div>
      {:else}
        <p class="muted">该轮次当前不可提交意见。</p>
      {/if}
    </div>
  {/each}
</div>

<style>
  .task { border-top: 1px solid #eee; padding-top: 12px; margin-top: 12px; }
  .form { display: flex; flex-direction: column; gap: 10px; max-width: 520px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 14px; }
  a { color: #24509e; }
</style>
