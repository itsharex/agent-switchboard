export const ANSWER = 'E2E desktop answer';
export const REASONING = 'E2E private reasoning';
export const TOOL_NAME = 'e2e_echo';

export function responseFor(protocol, body) {
  const tools = JSON.stringify(body).includes('E2E_TOOLS');
  if (protocol === 'responses') return responsesResponse(tools);
  if (protocol === 'chatCompletions') return chatResponse(tools);
  return anthropicResponse(tools);
}

function responsesResponse(tools) {
  const output = [
    { id: 'rs_e2e', type: 'reasoning', summary: [{ type: 'summary_text', text: REASONING }] },
    { id: 'msg_e2e', type: 'message', role: 'assistant', status: 'completed', content: [{ type: 'output_text', text: ANSWER, annotations: [] }] },
  ];
  if (tools) output.push({ id: 'fc_e2e', type: 'function_call', call_id: 'call_e2e', name: TOOL_NAME, arguments: '{"value":"hello"}', status: 'completed' });
  return { id: 'resp_e2e', object: 'response', created_at: 1, status: 'completed', model: 'e2e-model', output, usage: { input_tokens: 11, output_tokens: 7, total_tokens: 18 } };
}

function chatResponse(tools) {
  const message = { role: 'assistant', content: ANSWER, reasoning_content: REASONING };
  if (tools) message.tool_calls = [{ id: 'call_e2e', type: 'function', function: { name: TOOL_NAME, arguments: '{"value":"hello"}' } }];
  return { id: 'chat_e2e', object: 'chat.completion', created: 1, model: 'e2e-model', choices: [{ index: 0, message, finish_reason: tools ? 'tool_calls' : 'stop' }], usage: { prompt_tokens: 11, completion_tokens: 7, total_tokens: 18 } };
}

function anthropicResponse(tools) {
  const content = [{ type: 'thinking', thinking: REASONING, signature: 'e2e-signature' }, { type: 'text', text: ANSWER }];
  if (tools) content.push({ type: 'tool_use', id: 'call_e2e', name: TOOL_NAME, input: { value: 'hello' } });
  return { id: 'msg_e2e', type: 'message', role: 'assistant', model: 'e2e-model', content, stop_reason: tools ? 'tool_use' : 'end_turn', stop_sequence: null, usage: { input_tokens: 11, output_tokens: 7 } };
}

const sse = (event, value) => `${event ? `event: ${event}\n` : ''}data: ${JSON.stringify(value)}\n\n`;

export function streamFor(protocol, body) {
  const response = responseFor(protocol, body);
  if (protocol === 'chatCompletions') return chatStream(response);
  if (protocol === 'responses') return responsesStream(response);
  return anthropicStream(response);
}

function chatStream(response) {
  const chunk = (delta, finish_reason = null) => sse(null, { ...response, object: 'chat.completion.chunk', choices: [{ index: 0, delta, finish_reason }] });
  const message = response.choices[0].message;
  const events = [chunk({ role: 'assistant' }), chunk({ reasoning_content: REASONING }), chunk({ content: ANSWER })];
  if (message.tool_calls) events.push(chunk({ tool_calls: message.tool_calls.map((tool, index) => ({ index, ...tool })) }));
  events.push(chunk({}, response.choices[0].finish_reason), 'data: [DONE]\n\n');
  return events;
}

function responsesStream(response) {
  let sequence_number = 0;
  const event = (type, fields) => sse(type, { type, sequence_number: sequence_number++, ...fields });
  const events = [event('response.created', { response: { ...response, status: 'in_progress', output: [] } })];
  for (const [output_index, item] of response.output.entries()) {
    events.push(event('response.output_item.added', { output_index, item: { ...item, status: 'in_progress' } }));
    if (item.type === 'message') {
      events.push(event('response.content_part.added', { item_id: item.id, output_index, content_index: 0, part: { type: 'output_text', text: '', annotations: [] } }));
      events.push(event('response.output_text.delta', { item_id: item.id, output_index, content_index: 0, delta: ANSWER }));
      events.push(event('response.output_text.done', { item_id: item.id, output_index, content_index: 0, text: ANSWER }));
    } else if (item.type === 'reasoning') {
      events.push(event('response.reasoning_summary_text.delta', { item_id: item.id, output_index, summary_index: 0, delta: REASONING }));
    } else if (item.type === 'function_call') {
      events.push(event('response.function_call_arguments.delta', { item_id: item.id, output_index, delta: item.arguments }));
      events.push(event('response.function_call_arguments.done', { item_id: item.id, output_index, arguments: item.arguments }));
    }
    events.push(event('response.output_item.done', { output_index, item }));
  }
  events.push(event('response.completed', { response }));
  return events;
}

function anthropicStream(response) {
  const events = [sse('message_start', { type: 'message_start', message: { ...response, content: [], stop_reason: null, usage: { input_tokens: 11, output_tokens: 0 } } })];
  for (const [index, block] of response.content.entries()) {
    let initial, delta;
    if (block.type === 'thinking') {
      initial = { type: 'thinking', thinking: '' };
      delta = { type: 'thinking_delta', thinking: REASONING };
    } else if (block.type === 'text') {
      initial = { type: 'text', text: '' };
      delta = { type: 'text_delta', text: ANSWER };
    } else {
      initial = { ...block, input: {} };
      delta = { type: 'input_json_delta', partial_json: JSON.stringify(block.input) };
    }
    events.push(sse('content_block_start', { type: 'content_block_start', index, content_block: initial }));
    events.push(sse('content_block_delta', { type: 'content_block_delta', index, delta }));
    if (block.type === 'thinking') events.push(sse('content_block_delta', { type: 'content_block_delta', index, delta: { type: 'signature_delta', signature: 'e2e-signature' } }));
    events.push(sse('content_block_stop', { type: 'content_block_stop', index }));
  }
  events.push(sse('message_delta', { type: 'message_delta', delta: { stop_reason: response.stop_reason, stop_sequence: null }, usage: { output_tokens: 7 } }));
  events.push(sse('message_stop', { type: 'message_stop' }));
  return events;
}
