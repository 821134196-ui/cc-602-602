<script>
  import { api } from './api.js'
  let { onlogin } = $props()
  let username = $state('')
  let password = $state('')
  let error = $state('')

  async function submit() {
    error = ''
    try {
      const user = await api('/api/login', { method: 'POST', body: { username, password } })
      onlogin(user)
    } catch (e) { error = e.message }
  }

  const demo = [
    ['student1', '学生 陈晨'], ['student2', '学生 刘洋'],
    ['advisor1', '张明远 导师'], ['advisor2', '李慧 导师'],
    ['secretary', '王秘书'], ['reviewer1', '评阅人 赵'],
  ]
</script>

<div class="card login">
  <h2>登录</h2>
  <form onsubmit={(e) => { e.preventDefault(); submit() }}>
    <label>用户名 <input bind:value={username} required /></label>
    <label>密码 <input type="password" bind:value={password} required /></label>
    {#if error}<p class="error">{error}</p>{/if}
    <button type="submit">登录</button>
  </form>
  <p class="muted">演示账号（密码均为 password123）：</p>
  <div class="demo">
    {#each demo as [u, label]}
      <button class="ghost" onclick={() => { username = u; password = 'password123' }}>{label}</button>
    {/each}
  </div>
</div>

<style>
  .login { max-width: 420px; margin: 40px auto; }
  form { display: flex; flex-direction: column; gap: 12px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 14px; }
  .demo { display: flex; flex-wrap: wrap; gap: 8px; }
</style>
