#version 140

precision mediump float;

uniform mat4 projection;
uniform mat4 modelView;
uniform mat4 viewMatrix;

attribute vec3 position;
attribute vec3 normal;
attribute mat3 instancedNormalMatrix;
attribute mat4 instancedModelView;
attribute vec4 instancedColor;

varying vec3 vViewPosition;
varying vec3 vNormal;
varying vec4 vColor;
varying mat4 vInvertedView;

void main() {
  vNormal = normalize(instancedNormalMatrix * normal);
  vNormal.y = -vNormal.y;
  vInvertedView = inverse(viewMatrix);
  vColor = instancedColor;
  
  vec4 adjustedPosition = vec4(position, 1.0);
  vec4 mvPosition = modelView * instancedModelView * adjustedPosition;
  
  gl_Position = projection * viewMatrix * mvPosition;
  vViewPosition = -mvPosition.xyz;
}
