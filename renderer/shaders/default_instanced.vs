#version 140

precision mediump float;

uniform mat4 projection;
uniform mat4 modelView;
uniform mat4 viewMatrix;

attribute vec3 position;
attribute vec3 normal;
attribute mat4 instanceModelView;
attribute mat3 normalMatrix;
attribute vec4 color;

varying vec3 vViewPosition;
varying vec3 vNormal;
varying vec4 vColor;
varying mat4 vInvertedView;

void main() {
  vNormal = normalize(normalMatrix * normal);
  vInvertedView = inverse(viewMatrix);
  vColor = color;
  
  vec4 adjustedPosition = vec4(position, 1.0);
  vec4 mvPosition = modelView * instanceModelView * adjustedPosition;
  
  gl_Position = projection * viewMatrix * mvPosition;
  vViewPosition = -mvPosition.xyz;
}
