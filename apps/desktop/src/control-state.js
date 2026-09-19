// Transport failure does not revoke a lease. Only explicit keeper fencing does.
export async function probeControl(check,generation) {
  try {await check(generation);return {generation,state:'confirmed'};}
  catch(error) {return error.code==='controller-fenced'
    ? {generation:0,state:'fenced'} : {generation,state:'unconfirmed'};}
}
