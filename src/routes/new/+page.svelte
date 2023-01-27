<script lang="ts">
</script>

<div class="flex w-full p-1 gap-1">
  <div class="flex flex-col">
    <a class="btn btn-ghost btn-sm" href="/">
      Back
    </a>
    <button class="btn center btn-square btn-lg">
      <img src="/tauri.svg" class="w-12" alt="Icon">
    </button>
  </div>
  <div class="flex flex-col grow gap-1 form-control">
    <div class="form-control w-full">
      <label class="label" for="name">
        <span class="label-text">Name</span>
      </label>
      <input id="name" type="text" placeholder="1.19.3" class="input input-bordered w-full" />
    </div>
    <div class="form-control w-full">
      <label class="label" for="group">
        <span class="label-text">Group</span>
      </label>
      <input id="group" type="text" placeholder="Default" class="input input-bordered w-full" />
    </div>
  </div>
</div>

<style lang="postcss">

</style>